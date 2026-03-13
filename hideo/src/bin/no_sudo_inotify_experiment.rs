//! Experiment: run commands with overlayfs + inotify **without sudo**.
//!
//! This proves the no-sudo approach by replacing fanotify (which requires
//! `CAP_SYS_ADMIN` in the *initial* user namespace — see NOTES.md Finding 4)
//! with inotify (which has no such requirement).
//!
//! The trick:
//!   1. `unshare(CLONE_NEWUSER)` — creates a new user namespace where the
//!      current user has `CAP_SYS_ADMIN`.
//!   2. Map uid/gid so we appear as root inside the namespace.
//!   3. `unshare(CLONE_NEWNS)` — private mount namespace.
//!   4. Mount overlayfs with `userxattr` (supported since kernel 5.11).
//!   5. Use **inotify** instead of fanotify to watch the overlay.
//!
//! Limitations compared to fanotify:
//!   - inotify does **not** report the PID of the process that triggered the
//!     event (printed as `-` in output).
//!   - inotify requires a watch per directory (not per mount), so we walk the
//!     tree and install watches recursively.
//!   - Newly created directories are watched dynamically via `IN_CREATE`.
//!
//! Usage (**no sudo!**):
//!
//! ```sh
//! cargo run --bin no_sudo_experiment -- bash -c "touch x"
//! cargo run --bin no_sudo_experiment -- bash -c "cat Cargo.toml"
//! ```

use std::collections::HashMap;
use std::os::fd::AsFd;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use bpaf::Bpaf;
use nix::mount::{MntFlags, umount2};
use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
use nix::sched::{CloneFlags, unshare};
use nix::sys::inotify::{AddWatchFlags, InitFlags, Inotify, WatchDescriptor};
use nix::unistd::{Gid, Uid};
use sys_mount::Mount;
use tempfile::TempDir;
use tracing::{debug, error, info, warn};

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Bpaf)]
#[bpaf(options, version)]
/// No-sudo experiment – overlayfs + inotify in a user namespace.
struct Opts {
    /// Command to execute inside the overlay
    #[bpaf(positional("CMD"))]
    cmd: String,
    /// Arguments forwarded to CMD (flags like -c are passed through as-is)
    #[bpaf(any("ARGS", |s: String| Some(s)))]
    args: Vec<String>,
}

// ---------------------------------------------------------------------------
// Workspace detection
// ---------------------------------------------------------------------------

const WORKSPACE_MARKER: &str = "ZACK_WORKSPACE.star";

fn detect_workspace() -> Result<PathBuf> {
    let current_dir = std::env::current_dir().context("getting current directory")?;
    let mut current: &Path = &current_dir;
    loop {
        if current.join(WORKSPACE_MARKER).exists() {
            return Ok(current.to_path_buf());
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => anyhow::bail!(
                "No {WORKSPACE_MARKER} found in any parent of '{}'.\n\
                 Create one in your workspace root first.",
                current_dir.display()
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// User-namespace setup (the key to avoiding sudo)
// ---------------------------------------------------------------------------

/// Create a **user namespace** that grants `CAP_SYS_ADMIN` inside it, then map
/// the current uid/gid to root (0) so that subsequent `mount(2)` calls succeed.
///
/// This is the critical difference from the main `hideo` binary: by operating
/// inside a user namespace we no longer need real root privileges.
fn setup_user_namespace() -> Result<()> {
    let uid = Uid::current();
    let gid = Gid::current();
    info!("Current uid={uid}, gid={gid}");

    info!("Creating user namespace (no sudo required)");
    unshare(CloneFlags::CLONE_NEWUSER)
        .context("unshare(CLONE_NEWUSER) – this kernel may not support user namespaces")?;

    // `setgroups` must be set to "deny" before writing `gid_map` when the
    // process is unprivileged (see user_namespaces(7)).
    std::fs::write("/proc/self/setgroups", "deny\n").context("writing /proc/self/setgroups")?;

    // Map current uid → 0 inside the namespace.
    std::fs::write("/proc/self/uid_map", format!("0 {uid} 1\n"))
        .context("writing /proc/self/uid_map")?;

    // Map current gid → 0 inside the namespace.
    std::fs::write("/proc/self/gid_map", format!("0 {gid} 1\n"))
        .context("writing /proc/self/gid_map")?;

    info!("User namespace ready (mapped {uid}->0, {gid}->0)");
    Ok(())
}

// ---------------------------------------------------------------------------
// Mount-namespace setup
// ---------------------------------------------------------------------------

fn setup_mount_namespace() -> Result<()> {
    debug!("Creating private mount namespace");
    unshare(CloneFlags::CLONE_NEWNS).context("unshare(CLONE_NEWNS)")?;
    debug!("Mount namespace ready");
    Ok(())
}

// ---------------------------------------------------------------------------
// Overlayfs (with `userxattr` for unprivileged mounting)
// ---------------------------------------------------------------------------

struct Overlay {
    mount_point: PathBuf,
    _tmpdir: TempDir,
}

/// Mount an overlayfs whose **lower** (read-only) layer is the workspace.
///
/// Compared to the main `hideo` binary, this uses the `userxattr` mount option
/// so that overlayfs stores its metadata as `user.overlay.*` extended
/// attributes instead of `trusted.overlay.*`.  The `trusted.*` namespace
/// requires `CAP_SYS_ADMIN` in the *initial* namespace, whereas `user.*` is
/// writable from within a user namespace.
fn setup_overlayfs(workspace: &Path) -> Result<Overlay> {
    let tmpdir = TempDir::new().context("creating temp dir for overlayfs")?;

    let upper = tmpdir.path().join("upper");
    let work = tmpdir.path().join("work");
    let merged = tmpdir.path().join("merged");

    std::fs::create_dir_all(&upper).context("creating upper dir")?;
    std::fs::create_dir_all(&work).context("creating work dir")?;
    std::fs::create_dir_all(&merged).context("creating merged dir")?;

    let data = format!(
        "lowerdir={},upperdir={},workdir={},userxattr",
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
// Recursive inotify watcher
// ---------------------------------------------------------------------------

/// The set of inotify event flags we care about for each watched directory.
const WATCH_MASK: AddWatchFlags = AddWatchFlags::IN_OPEN
    .union(AddWatchFlags::IN_CLOSE_WRITE)
    .union(AddWatchFlags::IN_CLOSE_NOWRITE)
    .union(AddWatchFlags::IN_CREATE)
    .union(AddWatchFlags::IN_MOVED_TO);

struct RecursiveWatcher {
    inotify: Inotify,
    /// Maps each watch descriptor back to its directory path so we can resolve
    /// relative file names from events into full paths.
    watches: HashMap<WatchDescriptor, PathBuf>,
}

impl RecursiveWatcher {
    fn new() -> Result<Self> {
        let inotify = Inotify::init(InitFlags::IN_NONBLOCK | InitFlags::IN_CLOEXEC)
            .context("inotify_init")?;

        info!("inotify initialised (no CAP_SYS_ADMIN required)");

        Ok(Self {
            inotify,
            watches: HashMap::new(),
        })
    }

    /// Add a watch on `dir` and recurse into all subdirectories.
    fn watch_recursive(&mut self, dir: &Path) -> Result<()> {
        self.add_single_watch(dir)?;

        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => {
                debug!("Cannot read dir {}: {e}", dir.display());
                return Ok(());
            }
        };

        for entry in entries.flatten() {
            let ft = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };
            // Only recurse into real directories (skip symlinks to avoid loops).
            if ft.is_dir() {
                self.watch_recursive(&entry.path())?;
            }
        }

        Ok(())
    }

    fn add_single_watch(&mut self, dir: &Path) -> Result<WatchDescriptor> {
        let wd = self
            .inotify
            .add_watch(dir, WATCH_MASK)
            .with_context(|| format!("inotify_add_watch on {}", dir.display()))?;
        self.watches.insert(wd, dir.to_path_buf());
        Ok(wd)
    }

    /// Drain all pending inotify events and print them.
    ///
    /// Returns the number of user-visible events printed (excludes pure
    /// `IN_CREATE` directory-tracking events).
    fn drain_events(&mut self, overlay_root: &Path) -> usize {
        let mut total = 0;

        loop {
            let events = match self.inotify.read_events() {
                Ok(events) if events.is_empty() => break,
                Ok(events) => events,
                Err(nix::errno::Errno::EAGAIN) => break,
                Err(e) => {
                    warn!("Error reading inotify events: {e}");
                    break;
                }
            };

            // Collect new directories to watch – we cannot add watches while
            // iterating because `add_single_watch` borrows `self` mutably.
            let mut new_dirs: Vec<PathBuf> = Vec::new();

            for event in &events {
                let dir_path = self.watches.get(&event.wd).cloned();
                let file_name = event.name.as_deref();

                let full_path = match (&dir_path, file_name) {
                    (Some(dir), Some(name)) => Some(dir.join(name)),
                    _ => dir_path.clone(),
                };

                let rel_path = full_path
                    .as_ref()
                    .and_then(|p| p.strip_prefix(overlay_root).ok())
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "<unknown>".into());

                let mask = event.mask;

                // Dynamically watch newly created or moved-in directories.
                if mask.intersects(AddWatchFlags::IN_CREATE | AddWatchFlags::IN_MOVED_TO)
                    && mask.contains(AddWatchFlags::IN_ISDIR)
                {
                    if let Some(ref full) = full_path {
                        new_dirs.push(full.clone());
                    }
                }

                // Print user-facing events.  The `-` placeholder is where PID
                // would go — inotify does not provide it.
                if mask.contains(AddWatchFlags::IN_CLOSE_WRITE) {
                    println!("WRITE\t-\t{rel_path}");
                    total += 1;
                } else if mask.contains(AddWatchFlags::IN_OPEN) {
                    println!("OPEN\t-\t{rel_path}");
                    total += 1;
                } else if mask.contains(AddWatchFlags::IN_CLOSE_NOWRITE) {
                    println!("READ_CLOSE\t-\t{rel_path}");
                    total += 1;
                }
            }

            // Add watches for any newly created directories (recursively, in
            // case they already contain subdirectories — e.g. `mkdir -p`).
            for dir in new_dirs {
                if let Err(e) = self.watch_recursive(&dir) {
                    debug!("Failed to watch new dir {}: {e}", dir.display());
                }
            }
        }

        total
    }
}

// ---------------------------------------------------------------------------
// Main run loop
// ---------------------------------------------------------------------------

fn run(opts: Opts) -> Result<ExitCode> {
    // 1. Detect workspace root.
    let workspace = detect_workspace()?;
    info!("Workspace root: {}", workspace.display());

    // 2. Enter a user namespace — this gives us CAP_SYS_ADMIN without sudo.
    //    Finding 4 in NOTES.md showed that the overlay mount succeeds in a user
    //    namespace; only fanotify breaks.  By switching to inotify we sidestep
    //    that entirely.
    setup_user_namespace()?;

    // 3. Enter a private mount namespace so the overlay is invisible to the
    //    rest of the system.
    setup_mount_namespace()?;

    // 4. Mount overlayfs with `userxattr` (works in user namespace, unlike
    //    the `trusted.overlay.*` xattrs that require initial-ns CAP_SYS_ADMIN).
    let overlay = setup_overlayfs(&workspace)?;

    // 5. Set up recursive inotify watches on the overlay.
    let mut watcher = RecursiveWatcher::new()?;
    info!(
        "Installing recursive watches on {} …",
        overlay.mount_point.display()
    );
    watcher.watch_recursive(&overlay.mount_point)?;
    info!("Watching {} directories", watcher.watches.len());

    // 6. Spawn the requested command inside the overlay.
    info!("Spawning: {} {:?}", opts.cmd, opts.args);
    let mut child = std::process::Command::new(&opts.cmd)
        .args(&opts.args)
        .current_dir(&overlay.mount_point)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .with_context(|| format!("spawning command `{}`", opts.cmd))?;

    // 7. Poll inotify events while the child is running.
    //
    //    The poll + drain is done in two phases to avoid borrow conflicts:
    //    `poll` borrows the inotify fd immutably (via `as_fd`), while
    //    `drain_events` needs `&mut self` to add watches for new directories.
    //    We scope the immutable borrow inside a block so it is released before
    //    the mutable call.
    loop {
        let has_events = {
            let mut poll_fds = [PollFd::new(watcher.inotify.as_fd(), PollFlags::POLLIN)];
            match poll(&mut poll_fds, PollTimeout::from(100_u16)) {
                Ok(_) => {}
                Err(nix::errno::Errno::EINTR) => continue,
                Err(e) => {
                    warn!("poll error: {e}");
                }
            }
            poll_fds[0]
                .revents()
                .is_some_and(|r| r.contains(PollFlags::POLLIN))
        };

        if has_events {
            watcher.drain_events(&overlay.mount_point);
        }

        // Check whether the child has exited (non-blocking).
        match child.try_wait().context("waiting for child")? {
            Some(status) => {
                // One final drain so we don't miss late events.
                watcher.drain_events(&overlay.mount_point);

                // 8. Clean up.
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
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
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
