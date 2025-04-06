#![allow(unused)]

use camino::Utf8Path;
use nix::mount::{mount, umount2, MsFlags};
use nix::sched::{clone, CloneFlags};
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::chdir;
use nix::unistd::execvp;
use std::ffi::{CString, NulError};
use std::fs;

const STACK_SIZE: usize = 1024 * 1024;

impl crate::Exec {
    fn run(&self, root_dir: &Utf8Path) -> crate::Result<i32> {
        let mut stack: Vec<u8> = vec![0; STACK_SIZE];

        let child = unsafe {
            // FIXME: Use clone3
            clone(
                Box::new(|| {
                    self.run_child(root_dir);
                    // FIXME: Propagate exit code
                    0
                }),
                &mut stack,
                CloneFlags::empty(),
                Some(libc::SIGCHLD),
            ).map_err(crate::ExecError::SandboxClone)?
        };

        loop {
            match waitpid(child, None).map_err(crate::ExecError::SandboxWaitPid)? {
                WaitStatus::Exited(_, code) => return Ok(code),
                WaitStatus::Signaled(_, signal, _) => return Err(crate::ExecError::SandboxKilled(signal)),
                _ => continue,
            }
        }
    }

    // FIXME: Error handling etc.
    fn run_child(&self, root_dir: &Utf8Path)  {
        // === 1. Map UID/GID ===
        let pid = std::process::id();

        nix::sched::unshare(CloneFlags::CLONE_NEWUSER).expect("unshare failed");

        fs::write("/proc/self/uid_map", "0 1000 1").unwrap();
        fs::write("/proc/self/setgroups", "deny").unwrap();
        fs::write("/proc/self/gid_map", "0 1000 1").unwrap();

        nix::sched::unshare(CloneFlags::CLONE_NEWNS).expect("unshare failed");

        // === 2. Make mounts private ===
        mount(
            Some("none"),
            "/",
            None::<&str>,
            MsFlags::MS_REC | MsFlags::MS_PRIVATE,
            None::<&str>,
        )
            .expect("mount --make-rprivate failed");


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
        let options = format!(
            "lowerdir=/build-root,upperdir={},workdir={}",
            root_dir.as_str(),
            workdir.path().to_str().unwrap(),
        );

        mount(
            Some("overlay"),
            root_dir.as_str(),
            Some("overlay"),
            MsFlags::empty(),
            Some(options.as_str()),
        )
            .expect("overlay mount failed");

        let old_root = root_dir.join("old_root");
        fs::create_dir_all(&old_root).unwrap();

        // === 4. pivot_root ===
        chdir(root_dir.as_std_path()).unwrap();

        // bind mount proc
        mount(
            Some("/proc"),
            root_dir.join("/proc").as_str(),
            Some("proc"),
            MsFlags::MS_BIND,
            None::<&str>,
        )
            .expect("mount --bind failed");
            
        eprintln!("pivot_root: {} -> {}", root_dir, old_root);
        nix::unistd::pivot_root(root_dir.as_std_path(), old_root.as_std_path()).expect("pivot_root failed");
        chdir("/").unwrap();

        umount2("/old_root", nix::mount::MntFlags::MNT_DETACH).expect("umount oldroot failed");
        fs::remove_dir_all("/old_root").unwrap();

        // === 5. Mount /proc ===
        // std::fs::create_dir_all("/proc").expect("Failed to create /proc");
        // mount(
        //     Some("proc"),
        //     "/proc",
        //     Some("proc"),
        //     MsFlags::empty(),
        //     None::<&str>,
        // )
        //     .expect("mount /proc failed");

        // === 6. Exec process inside isolated container ===
        // FIXME
        let cmd = 
            self.command.iter().map(|s| CString::new(s.as_str())).collect::<Result<Vec<_>,NulError>>().expect("no nul in command");

        execvp(&cmd[0], &cmd).expect("exec failed");
    }
}

#[cfg(test)]
mod tests {
    use camino::Utf8Path;
    use crate::Exec;

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
