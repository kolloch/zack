use std::os::fd::{AsFd, AsRawFd};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use bpaf::Bpaf;
use nix::mount::{MntFlags, umount2};
use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
use nix::sched::{CloneFlags, unshare};
use nix::sys::fanotify::{EventFFlags, Fanotify, FanotifyEvent, InitFlags, MarkFlags, MaskFlags};
use sys_mount::Mount;
use tempfile::TempDir;
use tracing::{debug, error, info, warn};

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Bpaf)]
#[bpaf(options, version)]
/// Simple advanced build step executor – detects file reads and writes.
struct Opts {
    /// Command to execute inside the overlay
    #[bpaf(positional("CMD"))]
    cmd: String,
    /// Arguments forwarded to CMD
    #[bpaf(positional("ARGS"))]
    args: Vec<String>,
}

// ---------------------------------------------------------------------------
// Workspace detection
// ---------------------------------------------------------------------------

const WORKSPACE_MARKER: &str = "ZACK_WORKSPACE.star";

/// Walk up from the current directory until we find `ZACK_WORKSPACE.star`.
fn detect_workspace() -> Result<PathBuf> {
    let current_dir = std::env::current_dir().context("getting current directory")?;
    let mut current: &Path = &current_dir;

    loop {
        if current.join(WORKSPACE_MARKER).exists() {
            return Ok(current.to_path_buf());
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => {
                anyhow::bail!(
                    "No {WORKSPACE_MARKER} found in any parent of '{}'.\n\
                     Create one in your workspace root first.",
                    current_dir.display()
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Namespace helpers
// ---------------------------------------------------------------------------

/// Create a new **user namespace** (so we gain `CAP_SYS_ADMIN` inside it) and
/// immediately afterwards a new **mount namespace** so we can set up overlayfs
/// without affecting the host.
fn setup_namespaces() -> Result<()> {
    let uid = nix::unistd::getuid().as_raw();
    let gid = nix::unistd::getgid().as_raw();

    debug!("Creating user namespace (outer uid={uid}, gid={gid})");

    unshare(CloneFlags::CLONE_NEWUSER)
        .map_err(|e| anyhow::anyhow!("unshare(CLONE_NEWUSER): {e}"))?;

    // Must deny setgroups before writing gid_map (kernel requirement).
    std::fs::write("/proc/self/setgroups", "deny")
        .context("writing deny to /proc/self/setgroups")?;

    std::fs::write("/proc/self/uid_map", format!("0 {uid} 1\n"))
        .context("writing /proc/self/uid_map")?;
    std::fs::write("/proc/self/gid_map", format!("0 {gid} 1\n"))
        .context("writing /proc/self/gid_map")?;

    debug!("User namespace configured – creating mount namespace");

    unshare(CloneFlags::CLONE_NEWNS).map_err(|e| anyhow::anyhow!("unshare(CLONE_NEWNS): {e}"))?;

    debug!("Namespaces ready");
    Ok(())
}

// ---------------------------------------------------------------------------
// Overlayfs
// ---------------------------------------------------------------------------

struct Overlay {
    /// The directory where the overlay is mounted (combined view).
    mount_point: PathBuf,
    /// Keep the `TempDir` alive so it is cleaned up on drop.
    _tmpdir: TempDir,
}

/// Mount an overlayfs whose **lower** (read‑only) layer is the workspace.
/// Writes go to an ephemeral upper layer inside a temporary directory.
fn setup_overlayfs(workspace: &Path) -> Result<Overlay> {
    let tmpdir = TempDir::new().context("creating temp dir for overlayfs")?;

    let upper = tmpdir.path().join("upper");
    let work = tmpdir.path().join("work");
    let merged = tmpdir.path().join("merged");

    std::fs::create_dir_all(&upper).context("creating upper dir")?;
    std::fs::create_dir_all(&work).context("creating work dir")?;
    std::fs::create_dir_all(&merged).context("creating merged dir")?;

    let data = format!(
        "userxattr,lowerdir={},upperdir={},workdir={}",
        workspace.display(),
        upper.display(),
        work.display(),
    );

    debug!("Mounting overlayfs: {data}");

    Mount::builder()
        .fstype("overlay")
        .data(&data)
        .mount("overlay", &merged)
        .with_context(|| format!("mounting overlayfs with options: {data}"))?;

    info!("Overlay mounted at {}", merged.display());

    Ok(Overlay {
        mount_point: merged,
        _tmpdir: tmpdir,
    })
}

fn teardown_overlayfs(overlay: &Overlay) {
    if let Err(e) = umount2(&overlay.mount_point, MntFlags::MNT_DETACH) {
        warn!(
            "Failed to unmount overlay at {}: {e}",
            overlay.mount_point.display()
        );
    } else {
        debug!("Unmounted overlay at {}", overlay.mount_point.display());
    }
}

// ---------------------------------------------------------------------------
// Fanotify
// ---------------------------------------------------------------------------

/// Initialise fanotify and mark the given mount for open / close‑write events.
fn setup_fanotify(mount_point: &Path) -> Result<Fanotify> {
    let fan = Fanotify::init(
        InitFlags::FAN_CLASS_NOTIF | InitFlags::FAN_CLOEXEC | InitFlags::FAN_NONBLOCK,
        EventFFlags::O_RDONLY | EventFFlags::O_LARGEFILE | EventFFlags::O_CLOEXEC,
    )
    .context("fanotify_init")?;

    fan.mark(
        MarkFlags::FAN_MARK_ADD | MarkFlags::FAN_MARK_MOUNT,
        MaskFlags::FAN_OPEN | MaskFlags::FAN_CLOSE_WRITE | MaskFlags::FAN_CLOSE_NOWRITE,
        None::<i32>,
        Some(mount_point),
    )
    .context("fanotify_mark on overlay mount")?;

    info!("Fanotify watching mount at {}", mount_point.display());
    Ok(fan)
}

/// Drain all currently pending fanotify events and print them.
fn drain_events(fan: &Fanotify, overlay_root: &Path) {
    loop {
        match fan.read_events() {
            Ok(events) if events.is_empty() => break,
            Ok(events) => {
                for event in &events {
                    print_event(event, overlay_root);
                }
            }
            Err(nix::errno::Errno::EAGAIN) => break,
            Err(e) => {
                warn!("Error reading fanotify events: {e}");
                break;
            }
        }
    }
}

fn print_event(event: &FanotifyEvent, overlay_root: &Path) {
    if !event.check_version() {
        warn!("fanotify event version mismatch – skipping");
        return;
    }

    let mask = event.mask();
    let pid = event.pid();

    let kind = if mask.contains(MaskFlags::FAN_CLOSE_WRITE) {
        "WRITE"
    } else if mask.contains(MaskFlags::FAN_OPEN) {
        "OPEN"
    } else if mask.contains(MaskFlags::FAN_CLOSE_NOWRITE) {
        "READ_CLOSE"
    } else {
        "OTHER"
    };

    let path_display = event
        .fd()
        .and_then(|fd| resolve_fd_path(fd.as_raw_fd()).ok())
        .map(|abs| {
            abs.strip_prefix(overlay_root)
                .unwrap_or(&abs)
                .display()
                .to_string()
        })
        .unwrap_or_else(|| "<unknown>".into());

    println!("{kind}\t{pid}\t{path_display}");
}

fn resolve_fd_path(raw_fd: i32) -> Result<PathBuf> {
    let link = format!("/proc/self/fd/{raw_fd}");
    std::fs::read_link(&link).with_context(|| format!("readlink {link}"))
}

// ---------------------------------------------------------------------------
// Main run loop
// ---------------------------------------------------------------------------

fn run(opts: Opts) -> Result<ExitCode> {
    // 1. Detect workspace root.
    let workspace = detect_workspace()?;
    info!("Workspace root: {}", workspace.display());

    // 2. Enter user + mount namespaces so we can mount without real root.
    setup_namespaces()?;

    // 3. Mount overlayfs (workspace = lower / read-only layer).
    let overlay = setup_overlayfs(&workspace)?;

    // 4. Set up fanotify on the overlay mount.
    let fan = setup_fanotify(&overlay.mount_point)?;

    // 5. Spawn the requested command inside the overlay.
    info!("Spawning: {} {:?}", opts.cmd, opts.args);
    let mut child = std::process::Command::new(&opts.cmd)
        .args(&opts.args)
        .current_dir(&overlay.mount_point)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .with_context(|| format!("spawning command `{}`", opts.cmd))?;

    // 6. Poll fanotify events while the child is running.
    let fan_fd = fan.as_fd();
    loop {
        let mut poll_fds = [PollFd::new(fan_fd, PollFlags::POLLIN)];
        match poll(&mut poll_fds, PollTimeout::from(100_u16)) {
            Ok(_) => {}
            Err(nix::errno::Errno::EINTR) => continue,
            Err(e) => {
                warn!("poll error: {e}");
            }
        }

        if poll_fds[0]
            .revents()
            .is_some_and(|r| r.contains(PollFlags::POLLIN))
        {
            drain_events(&fan, &overlay.mount_point);
        }

        // Check whether the child has exited (non-blocking).
        match child.try_wait().context("waiting for child")? {
            Some(status) => {
                // One final drain so we don't miss late events.
                drain_events(&fan, &overlay.mount_point);

                // 7. Clean up.
                teardown_overlayfs(&overlay);

                if status.success() {
                    info!("Command finished successfully");
                    return Ok(ExitCode::SUCCESS);
                } else {
                    let code = status.code().unwrap_or(1) as u8;
                    error!("Command failed with status {status}");
                    return Ok(ExitCode::from(code));
                }
            }
            None => continue,
        }
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_file(true)
        .with_line_number(true)
        .init();

    let opts = opts().fallback_to_usage().run();

    match run(opts) {
        Ok(code) => code,
        Err(e) => {
            error!("{e:#}");
            ExitCode::FAILURE
        }
    }
}
