//! Experiment: FUSE passthrough with overlayfs **without sudo**.
//!
//! This implements the architecture from PLAN-fuse.md:
//!
//! ```text
//! command's cwd  →  FUSE passthrough        (records reads + writes, forwards ops)
//!                        ↓
//!                overlayfs                  (isolates writes from the real workspace)
//!                    lower: workspace       (read-only)
//!                    upper: tmpfs           (all writes land here)
//! ```
//!
//! The FUSE daemon logs every `open` and `release` (close), classifying each as
//! a read or write based on the flags.  All actual I/O is forwarded to the
//! backing overlayfs via the real filesystem.
//!
//! Like the inotify experiment, this uses `CLONE_NEWUSER` so **no sudo is
//! required**.  Unlike inotify, the FUSE approach gives us per-open PID
//! information.
//!
//! Usage:
//!
//! ```sh
//! cargo run --bin fuse_experiment -- bash -c "touch x"
//! cargo run --bin fuse_experiment -- bash -c "cat Cargo.toml"
//! ```

use std::collections::HashMap;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{FileExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use bpaf::Bpaf;
use fuser::{
    FileAttr, FileType, Filesystem, MountOption, ReplyAttr, ReplyCreate, ReplyData,
    ReplyDirectory, ReplyEmpty, ReplyEntry, ReplyOpen, ReplyWrite, Request, TimeOrNow,
};
use nix::mount::{MntFlags, umount2};
use nix::sched::{CloneFlags, unshare};
use nix::unistd::{Gid, Uid};
use sys_mount::Mount;
use tempfile::TempDir;
use tracing::{debug, error, info, warn};

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Bpaf)]
#[bpaf(options, version)]
/// FUSE passthrough experiment – overlayfs + FUSE in a user namespace.
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
// User-namespace setup (no sudo required)
// ---------------------------------------------------------------------------

fn setup_user_namespace() -> Result<()> {
    let uid = Uid::current();
    let gid = Gid::current();
    info!("Current uid={uid}, gid={gid}");

    info!("Creating user namespace (no sudo required)");
    unshare(CloneFlags::CLONE_NEWUSER)
        .context("unshare(CLONE_NEWUSER) – this kernel may not support user namespaces")?;

    std::fs::write("/proc/self/setgroups", "deny\n").context("writing /proc/self/setgroups")?;
    std::fs::write("/proc/self/uid_map", format!("0 {uid} 1\n"))
        .context("writing /proc/self/uid_map")?;
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
// Inode & file-handle bookkeeping
// ---------------------------------------------------------------------------

struct InodeStore {
    by_inode: HashMap<u64, PathBuf>,
    by_path: HashMap<PathBuf, u64>,
    next: u64,
}

impl InodeStore {
    fn new() -> Self {
        let mut store = Self {
            by_inode: HashMap::new(),
            by_path: HashMap::new(),
            next: 2, // inode 1 = root
        };
        let root = PathBuf::new();
        store.by_inode.insert(1, root.clone());
        store.by_path.insert(root, 1);
        store
    }

    fn get_or_insert(&mut self, path: PathBuf) -> u64 {
        if let Some(&ino) = self.by_path.get(&path) {
            return ino;
        }
        let ino = self.next;
        self.next += 1;
        self.by_inode.insert(ino, path.clone());
        self.by_path.insert(path, ino);
        ino
    }

    fn path(&self, ino: u64) -> Option<&Path> {
        self.by_inode.get(&ino).map(|p| p.as_path())
    }
}

struct OpenFile {
    file: std::fs::File,
    writable: bool,
    rel_path: PathBuf,
}

struct HandleStore {
    files: HashMap<u64, OpenFile>,
    next: u64,
}

impl HandleStore {
    fn new() -> Self {
        Self {
            files: HashMap::new(),
            next: 1,
        }
    }

    fn insert(&mut self, file: std::fs::File, writable: bool, rel_path: PathBuf) -> u64 {
        let fh = self.next;
        self.next += 1;
        self.files.insert(
            fh,
            OpenFile {
                file,
                writable,
                rel_path,
            },
        );
        fh
    }
}

// ---------------------------------------------------------------------------
// FUSE passthrough filesystem
// ---------------------------------------------------------------------------

const TTL: Duration = Duration::ZERO; // no kernel caching — every op goes through us

struct PassthroughFs {
    backing_root: PathBuf,
    inodes: InodeStore,
    handles: HandleStore,
}

impl PassthroughFs {
    fn new(backing_root: PathBuf) -> Self {
        Self {
            backing_root,
            inodes: InodeStore::new(),
            handles: HandleStore::new(),
        }
    }

    fn real_path(&self, rel: &Path) -> PathBuf {
        self.backing_root.join(rel)
    }

    fn stat(&self, ino: u64, rel: &Path) -> std::io::Result<FileAttr> {
        let meta = std::fs::symlink_metadata(self.real_path(rel))?;
        Ok(meta_to_attr(ino, &meta))
    }

    /// Display-friendly relative path (root shown as ".").
    fn display_rel(rel: &Path) -> String {
        if rel.as_os_str().is_empty() {
            ".".into()
        } else {
            rel.display().to_string()
        }
    }
}

fn meta_to_attr(ino: u64, m: &std::fs::Metadata) -> FileAttr {
    let kind = if m.is_dir() {
        FileType::Directory
    } else if m.file_type().is_symlink() {
        FileType::Symlink
    } else {
        FileType::RegularFile
    };
    FileAttr {
        ino,
        size: m.size(),
        blocks: m.blocks(),
        atime: unix_time(m.atime(), m.atime_nsec()),
        mtime: unix_time(m.mtime(), m.mtime_nsec()),
        ctime: unix_time(m.ctime(), m.ctime_nsec()),
        crtime: UNIX_EPOCH,
        kind,
        perm: (m.mode() & 0o7777) as u16,
        nlink: m.nlink() as u32,
        uid: m.uid(),
        gid: m.gid(),
        rdev: m.rdev() as u32,
        blksize: m.blksize() as u32,
        flags: 0,
    }
}

fn unix_time(secs: i64, nsecs: i64) -> SystemTime {
    if secs >= 0 {
        UNIX_EPOCH + Duration::new(secs as u64, nsecs as u32)
    } else {
        UNIX_EPOCH
    }
}

/// Convert `TimeOrNow` to a `libc::timespec` suitable for `utimensat`.
fn time_or_now_to_ts(t: TimeOrNow) -> libc::timespec {
    match t {
        TimeOrNow::Now => libc::timespec {
            tv_sec: 0,
            tv_nsec: libc::UTIME_NOW,
        },
        TimeOrNow::SpecificTime(st) => {
            let d = st.duration_since(UNIX_EPOCH).unwrap_or_default();
            libc::timespec {
                tv_sec: d.as_secs() as _,
                tv_nsec: d.subsec_nanos() as _,
            }
        }
    }
}

fn set_times(
    path: &Path,
    atime: Option<TimeOrNow>,
    mtime: Option<TimeOrNow>,
) -> std::io::Result<()> {
    let atime_ts = match atime {
        Some(t) => time_or_now_to_ts(t),
        None => libc::timespec {
            tv_sec: 0,
            tv_nsec: libc::UTIME_OMIT,
        },
    };
    let mtime_ts = match mtime {
        Some(t) => time_or_now_to_ts(t),
        None => libc::timespec {
            tv_sec: 0,
            tv_nsec: libc::UTIME_OMIT,
        },
    };
    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
    let times = [atime_ts, mtime_ts];
    let ret = unsafe { libc::utimensat(libc::AT_FDCWD, c_path.as_ptr(), times.as_ptr(), 0) };
    if ret == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

fn errno(e: &std::io::Error) -> i32 {
    e.raw_os_error().unwrap_or(libc::EIO)
}

// ---------------------------------------------------------------------------
// Filesystem trait implementation
// ---------------------------------------------------------------------------

impl Filesystem for PassthroughFs {
    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let parent_rel = match self.inodes.path(parent) {
            Some(p) => p.to_path_buf(),
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        let child_rel = parent_rel.join(name);
        let ino = self.inodes.get_or_insert(child_rel.clone());

        match self.stat(ino, &child_rel) {
            Ok(attr) => reply.entry(&TTL, &attr, 0),
            Err(e) => reply.error(errno(&e)),
        }
    }

    fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
        let rel = match self.inodes.path(ino) {
            Some(p) => p.to_path_buf(),
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        match self.stat(ino, &rel) {
            Ok(attr) => reply.attr(&TTL, &attr),
            Err(e) => reply.error(errno(&e)),
        }
    }

    fn setattr(
        &mut self,
        _req: &Request,
        ino: u64,
        mode: Option<u32>,
        _uid: Option<u32>,
        _gid: Option<u32>,
        size: Option<u64>,
        atime: Option<TimeOrNow>,
        mtime: Option<TimeOrNow>,
        _ctime: Option<SystemTime>,
        _fh: Option<u64>,
        _crtime: Option<SystemTime>,
        _chgtime: Option<SystemTime>,
        _bkuptime: Option<SystemTime>,
        _flags: Option<u32>,
        reply: ReplyAttr,
    ) {
        let rel = match self.inodes.path(ino) {
            Some(p) => p.to_path_buf(),
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        let real = self.real_path(&rel);

        if let Some(size) = size {
            match std::fs::OpenOptions::new().write(true).open(&real) {
                Ok(f) => {
                    if let Err(e) = f.set_len(size) {
                        reply.error(errno(&e));
                        return;
                    }
                }
                Err(e) => {
                    reply.error(errno(&e));
                    return;
                }
            }
        }

        if let Some(mode) = mode {
            if let Err(e) =
                std::fs::set_permissions(&real, std::fs::Permissions::from_mode(mode))
            {
                reply.error(errno(&e));
                return;
            }
        }

        if atime.is_some() || mtime.is_some() {
            if let Err(e) = set_times(&real, atime, mtime) {
                reply.error(errno(&e));
                return;
            }
        }

        match self.stat(ino, &rel) {
            Ok(attr) => reply.attr(&TTL, &attr),
            Err(e) => reply.error(errno(&e)),
        }
    }

    fn open(&mut self, req: &Request, ino: u64, flags: i32, reply: ReplyOpen) {
        let rel = match self.inodes.path(ino) {
            Some(p) => p.to_path_buf(),
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        let real = self.real_path(&rel);
        let writable = (flags & libc::O_ACCMODE) != libc::O_RDONLY;

        let mut oo = std::fs::OpenOptions::new();
        match flags & libc::O_ACCMODE {
            libc::O_RDONLY => {
                oo.read(true);
            }
            libc::O_WRONLY => {
                oo.write(true);
            }
            libc::O_RDWR => {
                oo.read(true).write(true);
            }
            _ => {
                oo.read(true);
            }
        }
        if flags & libc::O_APPEND != 0 {
            oo.append(true);
        }
        if flags & libc::O_TRUNC != 0 {
            oo.truncate(true);
        }

        match oo.open(&real) {
            Ok(file) => {
                println!("OPEN\t{}\t{}", req.pid(), Self::display_rel(&rel));
                let fh = self.handles.insert(file, writable, rel);
                reply.opened(fh, 0);
            }
            Err(e) => reply.error(errno(&e)),
        }
    }

    fn read(
        &mut self,
        _req: &Request,
        _ino: u64,
        fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyData,
    ) {
        let Some(open_file) = self.handles.files.get(&fh) else {
            reply.error(libc::EBADF);
            return;
        };
        let mut buf = vec![0u8; size as usize];
        match open_file.file.read_at(&mut buf, offset as u64) {
            Ok(n) => reply.data(&buf[..n]),
            Err(e) => reply.error(errno(&e)),
        }
    }

    fn write(
        &mut self,
        _req: &Request,
        _ino: u64,
        fh: u64,
        offset: i64,
        data: &[u8],
        _write_flags: u32,
        _flags: i32,
        _lock_owner: Option<u64>,
        reply: ReplyWrite,
    ) {
        let Some(open_file) = self.handles.files.get(&fh) else {
            reply.error(libc::EBADF);
            return;
        };
        match open_file.file.write_at(data, offset as u64) {
            Ok(n) => reply.written(n as u32),
            Err(e) => reply.error(errno(&e)),
        }
    }

    fn flush(
        &mut self,
        _req: &Request,
        _ino: u64,
        _fh: u64,
        _lock_owner: u64,
        reply: ReplyEmpty,
    ) {
        reply.ok();
    }

    fn release(
        &mut self,
        req: &Request,
        _ino: u64,
        fh: u64,
        _flags: i32,
        _lock_owner: Option<u64>,
        _flush: bool,
        reply: ReplyEmpty,
    ) {
        if let Some(of) = self.handles.files.remove(&fh) {
            let display = Self::display_rel(&of.rel_path);
            if of.writable {
                println!("WRITE\t{}\t{}", req.pid(), display);
            } else {
                println!("READ_CLOSE\t{}\t{}", req.pid(), display);
            }
        }
        reply.ok();
    }

    fn create(
        &mut self,
        req: &Request,
        parent: u64,
        name: &OsStr,
        mode: u32,
        _umask: u32,
        flags: i32,
        reply: ReplyCreate,
    ) {
        let parent_rel = match self.inodes.path(parent) {
            Some(p) => p.to_path_buf(),
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        let child_rel = parent_rel.join(name);
        let real = self.real_path(&child_rel);

        let mut oo = std::fs::OpenOptions::new();
        oo.write(true).create(true).mode(mode);
        if flags & libc::O_TRUNC != 0 {
            oo.truncate(true);
        }
        if flags & libc::O_EXCL != 0 {
            oo.create_new(true);
        }
        if flags & libc::O_ACCMODE == libc::O_RDWR {
            oo.read(true);
        }

        match oo.open(&real) {
            Ok(file) => {
                let ino = self.inodes.get_or_insert(child_rel.clone());
                println!("OPEN\t{}\t{}", req.pid(), Self::display_rel(&child_rel));
                let fh = self.handles.insert(file, true, child_rel.clone());

                match self.stat(ino, &child_rel) {
                    Ok(attr) => reply.created(&TTL, &attr, 0, fh, 0),
                    Err(e) => reply.error(errno(&e)),
                }
            }
            Err(e) => reply.error(errno(&e)),
        }
    }

    fn readdir(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        let rel = match self.inodes.path(ino) {
            Some(p) => p.to_path_buf(),
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        let real = self.real_path(&rel);

        let rd = match std::fs::read_dir(&real) {
            Ok(rd) => rd,
            Err(e) => {
                reply.error(errno(&e));
                return;
            }
        };

        let mut entries: Vec<(PathBuf, String, FileType)> = vec![
            (rel.clone(), ".".into(), FileType::Directory),
            (
                rel.parent().map(|p| p.to_path_buf()).unwrap_or_default(),
                "..".into(),
                FileType::Directory,
            ),
        ];
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let child_rel = rel.join(&name);
            let kind = e
                .file_type()
                .map(|ft| {
                    if ft.is_dir() {
                        FileType::Directory
                    } else if ft.is_symlink() {
                        FileType::Symlink
                    } else {
                        FileType::RegularFile
                    }
                })
                .unwrap_or(FileType::RegularFile);
            entries.push((child_rel, name, kind));
        }

        for (i, (child_rel, name, kind)) in entries.into_iter().enumerate().skip(offset as usize) {
            let child_ino = self.inodes.get_or_insert(child_rel);
            if reply.add(child_ino, (i + 1) as i64, kind, &name) {
                break; // buffer full
            }
        }
        reply.ok();
    }

    fn opendir(&mut self, _req: &Request, ino: u64, _flags: i32, reply: ReplyOpen) {
        if self.inodes.path(ino).is_some() {
            reply.opened(0, 0);
        } else {
            reply.error(libc::ENOENT);
        }
    }

    fn releasedir(
        &mut self,
        _req: &Request,
        _ino: u64,
        _fh: u64,
        _flags: i32,
        reply: ReplyEmpty,
    ) {
        reply.ok();
    }

    fn access(&mut self, _req: &Request, ino: u64, _mask: i32, reply: ReplyEmpty) {
        if self.inodes.path(ino).is_some() {
            reply.ok();
        } else {
            reply.error(libc::ENOENT);
        }
    }

    fn mkdir(
        &mut self,
        _req: &Request,
        parent: u64,
        name: &OsStr,
        mode: u32,
        _umask: u32,
        reply: ReplyEntry,
    ) {
        let parent_rel = match self.inodes.path(parent) {
            Some(p) => p.to_path_buf(),
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        let child_rel = parent_rel.join(name);
        let real = self.real_path(&child_rel);

        if let Err(e) = std::fs::create_dir(&real) {
            reply.error(errno(&e));
            return;
        }
        if let Err(e) =
            std::fs::set_permissions(&real, std::fs::Permissions::from_mode(mode))
        {
            reply.error(errno(&e));
            return;
        }

        let ino = self.inodes.get_or_insert(child_rel.clone());
        match self.stat(ino, &child_rel) {
            Ok(attr) => reply.entry(&TTL, &attr, 0),
            Err(e) => reply.error(errno(&e)),
        }
    }

    fn unlink(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEmpty) {
        let parent_rel = match self.inodes.path(parent) {
            Some(p) => p.to_path_buf(),
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        let real = self.real_path(&parent_rel.join(name));
        match std::fs::remove_file(&real) {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(errno(&e)),
        }
    }

    fn rmdir(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEmpty) {
        let parent_rel = match self.inodes.path(parent) {
            Some(p) => p.to_path_buf(),
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        let real = self.real_path(&parent_rel.join(name));
        match std::fs::remove_dir(&real) {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(errno(&e)),
        }
    }

    fn rename(
        &mut self,
        _req: &Request,
        parent: u64,
        name: &OsStr,
        newparent: u64,
        newname: &OsStr,
        _flags: u32,
        reply: ReplyEmpty,
    ) {
        let old_parent_rel = match self.inodes.path(parent) {
            Some(p) => p.to_path_buf(),
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        let new_parent_rel = match self.inodes.path(newparent) {
            Some(p) => p.to_path_buf(),
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        let old_real = self.real_path(&old_parent_rel.join(name));
        let new_real = self.real_path(&new_parent_rel.join(newname));
        match std::fs::rename(&old_real, &new_real) {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(errno(&e)),
        }
    }

    fn readlink(&mut self, _req: &Request, ino: u64, reply: ReplyData) {
        let rel = match self.inodes.path(ino) {
            Some(p) => p.to_path_buf(),
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        let real = self.real_path(&rel);
        match std::fs::read_link(&real) {
            Ok(target) => reply.data(target.as_os_str().as_bytes()),
            Err(e) => reply.error(errno(&e)),
        }
    }

    fn statfs(&mut self, _req: &Request, _ino: u64, reply: fuser::ReplyStatfs) {
        // Return a reasonable default so programs that check free space don't fail.
        reply.statfs(0, 0, 0, 0, 0, 512, 255, 0);
    }
}

// ---------------------------------------------------------------------------
// Main run loop
// ---------------------------------------------------------------------------

fn run(opts: Opts) -> Result<ExitCode> {
    // 1. Detect workspace root.
    let workspace = detect_workspace()?;
    info!("Workspace root: {}", workspace.display());

    // 2. Enter a user namespace — CAP_SYS_ADMIN without sudo.
    setup_user_namespace()?;

    // 3. Private mount namespace.
    setup_mount_namespace()?;

    // 4. Mount overlayfs with userxattr.
    let overlay = setup_overlayfs(&workspace)?;

    // 5. Create a separate directory for the FUSE mount point.
    let fuse_dir = overlay._tmpdir.path().join("fuse");
    std::fs::create_dir_all(&fuse_dir).context("creating FUSE mount point")?;

    // 6. Check /dev/fuse is accessible.
    if !Path::new("/dev/fuse").exists() {
        anyhow::bail!(
            "/dev/fuse not found. FUSE support requires /dev/fuse to be available.\n\
             In Docker, run with `--device /dev/fuse` or `--privileged`."
        );
    }

    // 7. Mount FUSE passthrough on top of the overlay.
    info!("Mounting FUSE at {}", fuse_dir.display());
    let fs = PassthroughFs::new(overlay.mount_point.clone());
    let session = fuser::spawn_mount2(
        fs,
        &fuse_dir,
        &[
            MountOption::FSName("hideo-fuse".into()),
            MountOption::DefaultPermissions,
        ],
    )
    .context("mounting FUSE filesystem")?;

    // Give the FUSE mount a moment to become available.
    for i in 0..20 {
        match std::fs::metadata(&fuse_dir) {
            Ok(_) => break,
            Err(_) if i < 19 => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => anyhow::bail!("FUSE mount not ready: {e}"),
        }
    }
    info!("FUSE mounted at {}", fuse_dir.display());

    // 8. Spawn the requested command with cwd inside the FUSE mount.
    info!("Spawning: {} {:?}", opts.cmd, opts.args);
    let mut child = std::process::Command::new(&opts.cmd)
        .args(&opts.args)
        .current_dir(&fuse_dir)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .with_context(|| format!("spawning command `{}`", opts.cmd))?;

    // 9. Wait for the child to finish.
    let status = child.wait().context("waiting for child")?;

    // 10. Clean up: drop FUSE session first, then unmount overlay.
    drop(session);
    teardown_overlayfs(&overlay);

    if status.success() {
        info!("Command finished successfully");
        Ok(ExitCode::SUCCESS)
    } else {
        let code = status.code().unwrap_or(1) as u8;
        error!("Command failed with status {status}");
        Ok(ExitCode::from(code))
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
