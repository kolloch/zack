#![allow(unused)]

use camino::Utf8Path;
use nix::NixPath;
use nix::mount::{MsFlags, mount, umount2};
use nix::sched::{CloneFlags, clone};
use nix::sys::wait::{WaitStatus, waitpid};
use nix::unistd::chdir;
use nix::unistd::execvp;
use std::convert::Infallible;
use std::ffi::{CStr, CString, NulError};
use std::fs::{self, File};
use std::os::fd::FromRawFd;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command};
use tracing::{error, warn};
use std::io::Write;

impl crate::Exec {
    fn run(&self, root_dir: &Utf8Path) -> crate::Result<i32> {
        // FIXME: use clone3, and is this the stack only for the prelude?
        const STACK_SIZE: usize = 1024 * 1024;
        let mut stack: Vec<u8> = vec![0; STACK_SIZE];

        println!("sandbox.rs: parent process");

        let child = unsafe {
            // FIXME: Use clone3
            clone(
                Box::new(|| {
                    use std::os::fd::FromRawFd;
                    // Using println was problematic...
                    let mut stderr: File = unsafe { File::from_raw_fd(2) };
                    writeln!(stderr, "sandbox.rs: child process");
                    let _ = stderr.flush();
                    match self.run_child(root_dir) {
                        Err(err) => {
                            writeln!(
                                stderr,
                                "sandbox.rs: error while trying to run sandboxed child: {err:#}"
                            );
                            let _ = stderr.flush().unwrap();
                            127
                        }
                        Ok(_) => {
                            writeln!(stderr, "sandbox.rs: child process exited before exec");
                            let _ = stderr.flush().unwrap();
                            123
                        }
                    }
                }),
                &mut stack,
                CloneFlags::CLONE_FILES,
                Some(libc::SIGCHLD),
            )
            .map_err(crate::ExecError::SandboxClone)?
        };

        loop {
            match waitpid(child, None).map_err(crate::ExecError::SandboxWaitPid)? {
                WaitStatus::Exited(_, code) => return Ok(code),
                WaitStatus::Signaled(_, signal, _) => {
                    return Err(crate::ExecError::SandboxKilled(signal));
                }
                _ => continue,
            }
        }
    }

    fn unshare(clone_flags: CloneFlags) -> crate::Result<()> {
        nix::sched::unshare(clone_flags)
            .map_err(|e| crate::ExecError::SandboxUnshare(clone_flags, e))
    }

    fn mount(
        source: impl Into<Option<PathBuf>>,
        target: impl Into<PathBuf>,
        fstype: impl Into<Option<String>>,
        flags: MsFlags,
        data: impl Into<Option<String>>,
    ) -> crate::Result<()> {
        let source = source.into();
        let target = target.into();
        let fstype = fstype.into();
        let data = data.into();
        mount(
            source.as_ref(),
            &target,
            fstype.as_ref().map(|s| s as &str),
            flags,
            data.as_ref().map(|s| s as &str),
        )
        .map_err(|err| crate::ExecError::SandboxMount {
            err,
            source,
            target,
            fstype,
            flags,
            data,
        })
    }

    // FIXME: Error handling etc.
    fn run_child(&self, root_dir: &Utf8Path) -> crate::Result<Infallible> {
        // === 1. Map UID/GID ===
        let pid = std::process::id();

        Self::unshare(CloneFlags::CLONE_NEWUSER)?;
        let mut stderr: File = unsafe { File::from_raw_fd(2) };

        if let Err(err) = fs::write("/proc/self/uid_map", "0 1000 1") {
            writeln!(stderr, "while writing to /proc/self/uid_map: {err:#}");
        }
        if let Err(err) = fs::write("/proc/self/setgroups", "deny") {
            writeln!(stderr, "while writing to /proc/self/setgroups: {err:#}");
        }
        if let Err(err) = fs::write("/proc/self/gid_map", "0 1000 1") {
            writeln!(stderr, "while writing to /proc/self/gid_map: {err:#}");
        }

        let _ = stderr.flush();

        Self::unshare(CloneFlags::CLONE_NEWNS)?;

        // === 2. Make mounts private ===
        Self::mount(
            Some("none".into()),
            "/",
            None,
            MsFlags::MS_REC | MsFlags::MS_PRIVATE,
            None,
        )?;

        // bind mount the new root on itself, so we can use pivot_root
        // mount(
        //     Some(root_dir.as_str()),
        //     root_dir.as_str(),
        //     None::<&str>,
        //     MsFlags::MS_BIND,
        //     None::<&str>,
        // )
        //     .expect("mount --bind failed");

        // Use an overlay filesystem instead of bind mount, with /build-root as the lowerdir
        // and the new root as the upperdir
        let workdir = tempfile::tempdir().unwrap();
        // FIXME: Quoting
        let options = format!(
            "lowerdir=/build-root,upperdir={},workdir={}",
            root_dir.as_str(),
            workdir.path().to_str().unwrap(),
        );

        Self::mount(
            None,
            root_dir.as_str(),
            Some("overlay".into()),
            MsFlags::empty(),
            Some(options),
        )?;

        let old_root = root_dir.join("old_root");
        fs::create_dir_all(&old_root)?;

        std::env::set_current_dir(root_dir)?;

        chdir(root_dir.as_std_path())
            .map_err(|e| crate::ExecError::SandboxChdir(root_dir.into(), e))?;

        // bind mount proc
        Self::mount(
            Some("/proc".into()),
            root_dir.join("/proc").as_str(),
            Some("proc".into()),
            MsFlags::MS_BIND,
            None,
        )?;

        writeln!(stderr, "pivot_root: {} -> {}", root_dir, old_root);
        let _ = stderr.flush();
        nix::unistd::pivot_root(root_dir.as_std_path(), old_root.as_std_path())
            .map_err(|e| crate::ExecError::SandboxPivotRoot(e))?;
        std::env::set_current_dir("/")?;

        umount2("/old_root", nix::mount::MntFlags::MNT_DETACH).map_err(|e| crate::ExecError::SandboxUmount("/old_root".into(), e))?;
        fs::remove_dir_all("/old_root")?;

        Err(Command::new(&self.command[0])
            .args(&self.command[1..])
            .exec()
            .into())
    }
}

#[cfg(test)]
mod tests {
    use crate::Exec;
    use camino::Utf8Path;

    #[test]
    fn exec_true() -> crate::Result<()> {
        let exec = Exec {
            command: vec!["/usr/bin/true".into()],
            ..Default::default()
        };
        let exec_dir = tempfile::tempdir()?;
        let ret = exec.run(&Utf8Path::from_path(exec_dir.path()).unwrap())?;
        assert_eq!(ret, 0);
        Ok(())
    }

    #[test]
    fn exec_false() -> crate::Result<()> {
        let exec = Exec {
            command: vec!["/usr/bin/false".into()],
            ..Default::default()
        };
        let exec_dir = tempfile::tempdir()?;
        let ret = exec.run(&Utf8Path::from_path(exec_dir.path()).unwrap())?;
        assert_eq!(ret, 1);
        Ok(())
    }
}
