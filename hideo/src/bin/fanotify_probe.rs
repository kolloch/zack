//! Probe: does fanotify work on the filesystem at a given path?
//!
//! This is deliberately minimal — no overlayfs, no user namespaces, no tmpfs.
//! It answers one question: can fanotify watch the mount that backs a directory?
//!
//! Usage:
//!
//!   # Test the virtiofs workspace mount (default):
//!   sudo ./target/debug/fanotify_probe
//!
//!   # Test an explicit path:
//!   sudo ./target/debug/fanotify_probe /some/other/dir
//!
//! Exit codes:
//!   0  all expected events were received
//!   1  fanotify_init or fanotify_mark failed, OR no write event was detected

use std::os::fd::{AsFd, AsRawFd};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
use nix::sys::fanotify::{EventFFlags, Fanotify, FanotifyEvent, InitFlags, MarkFlags, MaskFlags};

// ---------------------------------------------------------------------------
// Filesystem info helpers
// ---------------------------------------------------------------------------

/// Return the fstype string for the mount that contains `path`.
fn fstype_of(path: &Path) -> String {
    // Parse /proc/self/mounts and find the longest prefix match.
    let Ok(mounts) = std::fs::read_to_string("/proc/self/mounts") else {
        return "<unknown>".into();
    };

    let mut best_prefix = "";
    let mut best_fstype = "<unknown>";

    let path_str = path.to_string_lossy();

    for line in mounts.lines() {
        // Fields: device mountpoint fstype options dump pass
        let mut fields = line.splitn(6, ' ');
        let _ = fields.next(); // device
        let mountpoint = fields.next().unwrap_or("");
        let fstype = fields.next().unwrap_or("");

        if path_str.starts_with(mountpoint) && mountpoint.len() > best_prefix.len() {
            best_prefix = mountpoint;
            best_fstype = fstype;
        }
    }

    best_fstype.to_owned()
}

// ---------------------------------------------------------------------------
// Probe steps
// ---------------------------------------------------------------------------

struct StepResult {
    label: &'static str,
    outcome: Result<String>,
}

impl StepResult {
    fn ok(label: &'static str, detail: impl Into<String>) -> Self {
        Self {
            label,
            outcome: Ok(detail.into()),
        }
    }
    fn err(label: &'static str, e: anyhow::Error) -> Self {
        Self {
            label,
            outcome: Err(e),
        }
    }
}

fn run_probe(target: &Path) -> Vec<StepResult> {
    let mut results: Vec<StepResult> = Vec::new();

    // ---- Step 1: report filesystem type ------------------------------------
    let fstype = fstype_of(target);
    results.push(StepResult::ok(
        "filesystem type",
        format!("{} (path: {})", fstype, target.display()),
    ));

    // ---- Step 2: fanotify_init ---------------------------------------------
    let fan = match Fanotify::init(
        InitFlags::FAN_CLASS_NOTIF | InitFlags::FAN_CLOEXEC | InitFlags::FAN_NONBLOCK,
        EventFFlags::O_RDONLY | EventFFlags::O_LARGEFILE | EventFFlags::O_CLOEXEC,
    )
    .context("fanotify_init")
    {
        Ok(f) => {
            results.push(StepResult::ok("fanotify_init", "fd opened"));
            f
        }
        Err(e) => {
            results.push(StepResult::err("fanotify_init", e));
            return results;
        }
    };

    // ---- Step 3: fanotify_mark (FAN_MARK_MOUNT) ----------------------------
    match fan
        .mark(
            MarkFlags::FAN_MARK_ADD | MarkFlags::FAN_MARK_MOUNT,
            MaskFlags::FAN_OPEN | MaskFlags::FAN_CLOSE_WRITE | MaskFlags::FAN_CLOSE_NOWRITE,
            None::<i32>,
            Some(target),
        )
        .with_context(|| format!("fanotify_mark(FAN_MARK_MOUNT) on {}", target.display()))
    {
        Ok(()) => {
            results.push(StepResult::ok(
                "fanotify_mark",
                format!("FAN_MARK_MOUNT on {}", target.display()),
            ));
        }
        Err(e) => {
            results.push(StepResult::err("fanotify_mark", e));
            return results;
        }
    }

    // ---- Step 4: write a probe file ----------------------------------------
    let probe_path = target.join("fanotify_probe_tmp.txt");

    match std::fs::write(&probe_path, b"fanotify probe\n")
        .with_context(|| format!("writing {}", probe_path.display()))
    {
        Ok(()) => {
            results.push(StepResult::ok(
                "write probe file",
                probe_path.display().to_string(),
            ));
        }
        Err(e) => {
            results.push(StepResult::err("write probe file", e));
            return results;
        }
    }

    // Also read it back to generate a CLOSE_NOWRITE event.
    let _ = std::fs::read(&probe_path);

    // Clean up the probe file regardless of what happens next.
    let _ = std::fs::remove_file(&probe_path);

    // ---- Step 5: collect events --------------------------------------------
    let counts = poll_and_collect(&fan, target, 5 /* rounds × 200 ms = 1 s */);

    results.push(StepResult::ok(
        "events received",
        format!(
            "OPEN={} CLOSE_WRITE={} CLOSE_NOWRITE={}",
            counts.open, counts.close_write, counts.close_nowrite
        ),
    ));

    // ---- Step 6: verdict ---------------------------------------------------
    if counts.open > 0 && counts.close_write > 0 {
        results.push(StepResult::ok(
            "verdict",
            "PASS — fanotify detected writes on this filesystem",
        ));
    } else {
        let mut missing = Vec::new();
        if counts.open == 0 {
            missing.push("OPEN");
        }
        if counts.close_write == 0 {
            missing.push("CLOSE_WRITE");
        }
        results.push(StepResult::err(
            "verdict",
            anyhow::anyhow!(
                "FAIL — missing events: {} (fanotify_mark succeeded but no events arrived)",
                missing.join(", ")
            ),
        ));
    }

    results
}

// ---------------------------------------------------------------------------
// Event helpers
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Counts {
    open: usize,
    close_write: usize,
    close_nowrite: usize,
}

fn poll_and_collect(fan: &Fanotify, mount_root: &Path, rounds: usize) -> Counts {
    let fan_fd = fan.as_fd();
    let mut counts = Counts::default();

    for _ in 0..rounds {
        let mut fds = [PollFd::new(fan_fd, PollFlags::POLLIN)];
        match poll(&mut fds, PollTimeout::from(200_u16)) {
            Ok(_) | Err(nix::errno::Errno::EINTR) => {}
            Err(e) => {
                eprintln!("  poll error: {e}");
                break;
            }
        }
        if fds[0]
            .revents()
            .is_some_and(|r| r.contains(PollFlags::POLLIN))
        {
            drain_events(fan, mount_root, &mut counts);
        }
    }

    counts
}

fn drain_events(fan: &Fanotify, mount_root: &Path, counts: &mut Counts) {
    loop {
        match fan.read_events() {
            Ok(events) if events.is_empty() => break,
            Ok(events) => {
                for ev in &events {
                    print_event(ev, mount_root, counts);
                }
            }
            Err(nix::errno::Errno::EAGAIN) => break,
            Err(e) => {
                eprintln!("  read_events error: {e}");
                break;
            }
        }
    }
}

fn print_event(event: &FanotifyEvent, mount_root: &Path, counts: &mut Counts) {
    if !event.check_version() {
        eprintln!("  event version mismatch");
        return;
    }

    let mask = event.mask();
    let pid = event.pid();

    let path = event
        .fd()
        .and_then(|fd| std::fs::read_link(format!("/proc/self/fd/{}", fd.as_raw_fd())).ok())
        .map(|abs| {
            abs.strip_prefix(mount_root)
                .unwrap_or(&abs)
                .display()
                .to_string()
        })
        .unwrap_or_else(|| "<unknown>".into());

    if mask.contains(MaskFlags::FAN_CLOSE_WRITE) {
        counts.close_write += 1;
        println!("  event: CLOSE_WRITE  pid={pid}  path={path}");
    }
    if mask.contains(MaskFlags::FAN_OPEN) {
        counts.open += 1;
        println!("  event: OPEN         pid={pid}  path={path}");
    }
    if mask.contains(MaskFlags::FAN_CLOSE_NOWRITE) {
        counts.close_nowrite += 1;
        println!("  event: CLOSE_NOWRITE pid={pid}  path={path}");
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() -> ExitCode {
    let target: PathBuf = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().expect("current_dir"));

    println!("fanotify probe");
    println!("target: {}", target.display());
    println!();

    let results = run_probe(&target);

    let mut any_failure = false;

    for (i, r) in results.iter().enumerate() {
        let step = i + 1;
        match &r.outcome {
            Ok(detail) => println!("[{step}] ✓  {}  —  {detail}", r.label),
            Err(e) => {
                println!("[{step}] ✗  {}  —  {e:#}", r.label);
                any_failure = true;
            }
        }
    }

    println!();

    if any_failure {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
