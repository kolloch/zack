//! Experiment: mount a fresh tmpfs, attach fanotify to it, write a file, and
//! verify the write event is detected.
//!
//! Run with:
//!
//!   RUST_LOG=info cargo run --bin tmpfs_experiment
//!
//! The binary exits 0 on success and 1 on failure.

use std::os::fd::{AsFd, AsRawFd};
use std::path::Path;
use std::process::ExitCode;

use anyhow::{Context, Result};
use nix::mount::{MntFlags, umount2};
use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
use nix::sys::fanotify::{EventFFlags, Fanotify, FanotifyEvent, InitFlags, MarkFlags, MaskFlags};
use sys_mount::Mount;
use tempfile::TempDir;
use tracing::{debug, error, info};

// ---------------------------------------------------------------------------
// tmpfs helpers
// ---------------------------------------------------------------------------

struct TmpfsMount {
    /// Directory at which tmpfs is mounted.
    mount_point: std::path::PathBuf,
    /// Keep alive so the directory is not deleted before we unmount.
    _tmpdir: TempDir,
}

impl TmpfsMount {
    fn mount() -> Result<Self> {
        let tmpdir = TempDir::new().context("creating host temp dir")?;
        let mount_point = tmpdir.path().to_path_buf();

        info!("Mounting tmpfs at {}", mount_point.display());

        Mount::builder()
            .fstype("tmpfs")
            .mount("tmpfs", &mount_point)
            .context("mounting tmpfs")?;

        info!("tmpfs mounted successfully");
        Ok(Self {
            mount_point,
            _tmpdir: tmpdir,
        })
    }

    fn path(&self) -> &Path {
        &self.mount_point
    }
}

impl Drop for TmpfsMount {
    fn drop(&mut self) {
        debug!("Unmounting tmpfs at {}", self.mount_point.display());
        if let Err(e) = umount2(&self.mount_point, MntFlags::MNT_DETACH) {
            error!("Failed to unmount tmpfs: {e}");
        } else {
            debug!("tmpfs unmounted");
        }
    }
}

// ---------------------------------------------------------------------------
// fanotify helpers
// ---------------------------------------------------------------------------

fn init_fanotify(mount_point: &Path) -> Result<Fanotify> {
    let fan = Fanotify::init(
        InitFlags::FAN_CLASS_NOTIF | InitFlags::FAN_CLOEXEC | InitFlags::FAN_NONBLOCK,
        EventFFlags::O_RDONLY | EventFFlags::O_LARGEFILE | EventFFlags::O_CLOEXEC,
    )
    .context("fanotify_init")?;

    info!("fanotify initialised");

    fan.mark(
        MarkFlags::FAN_MARK_ADD | MarkFlags::FAN_MARK_MOUNT,
        MaskFlags::FAN_OPEN | MaskFlags::FAN_CLOSE_WRITE | MaskFlags::FAN_CLOSE_NOWRITE,
        None::<i32>,
        Some(mount_point),
    )
    .with_context(|| format!("fanotify_mark on {}", mount_point.display()))?;

    info!("fanotify watching mount at {}", mount_point.display());

    Ok(fan)
}

// ---------------------------------------------------------------------------
// Event collection
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct Counts {
    open: usize,
    close_write: usize,
    close_nowrite: usize,
}

fn collect_events(fan: &Fanotify, mount_root: &Path, counts: &mut Counts) {
    loop {
        match fan.read_events() {
            Ok(events) if events.is_empty() => break,
            Ok(events) => {
                for ev in &events {
                    tally_event(ev, mount_root, counts);
                }
            }
            Err(nix::errno::Errno::EAGAIN) => break,
            Err(e) => {
                error!("read_events error: {e}");
                break;
            }
        }
    }
}

fn tally_event(event: &FanotifyEvent, mount_root: &Path, counts: &mut Counts) {
    if !event.check_version() {
        error!("fanotify event version mismatch – skipping");
        return;
    }

    let mask = event.mask();
    let pid = event.pid();

    let path_display = event
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
        info!("EVENT  CLOSE_WRITE  pid={pid}  path={path_display}");
    }
    if mask.contains(MaskFlags::FAN_OPEN) {
        counts.open += 1;
        info!("EVENT  OPEN         pid={pid}  path={path_display}");
    }
    if mask.contains(MaskFlags::FAN_CLOSE_NOWRITE) {
        counts.close_nowrite += 1;
        info!("EVENT  CLOSE_NOWRITE pid={pid}  path={path_display}");
    }
}

/// Poll fanotify for up to `rounds * 200 ms` and drain after each wake-up.
fn poll_and_collect(fan: &Fanotify, mount_root: &Path, rounds: usize) -> Counts {
    let fan_fd = fan.as_fd();
    let mut counts = Counts::default();

    for _ in 0..rounds {
        let mut fds = [PollFd::new(fan_fd, PollFlags::POLLIN)];
        match poll(&mut fds, PollTimeout::from(200_u16)) {
            Ok(_) | Err(nix::errno::Errno::EINTR) => {}
            Err(e) => {
                error!("poll error: {e}");
                break;
            }
        }
        if fds[0]
            .revents()
            .is_some_and(|r| r.contains(PollFlags::POLLIN))
        {
            collect_events(fan, mount_root, &mut counts);
        }
    }

    counts
}

// ---------------------------------------------------------------------------
// Experiment
// ---------------------------------------------------------------------------

fn run_experiment() -> Result<bool> {
    // ---- Step 1: mount tmpfs -----------------------------------------------
    let tmpfs = TmpfsMount::mount()?;

    // ---- Step 2: init fanotify on the tmpfs mount --------------------------
    let fan = init_fanotify(tmpfs.path())?;

    // ---- Step 3: write a file and read it back -----------------------------
    let test_file = tmpfs.path().join("probe.txt");
    info!("Writing test file: {}", test_file.display());
    std::fs::write(&test_file, b"fanotify tmpfs probe\n").context("writing test file")?;
    info!("Test file written");

    // Also read it back so we get a CLOSE_NOWRITE event.
    let _contents = std::fs::read(&test_file).context("reading test file back")?;
    info!("Test file read back");

    // ---- Step 4: collect events --------------------------------------------
    info!("Polling for fanotify events (up to ~1 s)…");
    let counts = poll_and_collect(&fan, tmpfs.path(), 5);

    // `tmpfs` is dropped here, which unmounts cleanly.
    drop(tmpfs);

    // ---- Step 5: evaluate --------------------------------------------------
    info!(
        "Results: OPEN={} CLOSE_WRITE={} CLOSE_NOWRITE={}",
        counts.open, counts.close_write, counts.close_nowrite
    );

    let write_detected = counts.close_write > 0;
    let open_detected = counts.open > 0;

    if write_detected && open_detected {
        println!("✓ PASS – fanotify detected writes and opens on tmpfs");
        Ok(true)
    } else {
        let mut issues = Vec::new();
        if !open_detected {
            issues.push("no OPEN event detected");
        }
        if !write_detected {
            issues.push("no CLOSE_WRITE event detected");
        }
        println!("✗ FAIL – {}", issues.join(", "));
        Ok(false)
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    info!("=== tmpfs + fanotify experiment ===");

    match run_experiment() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            error!("{e:#}");
            ExitCode::FAILURE
        }
    }
}
