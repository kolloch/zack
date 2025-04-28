use std::fs::{File, create_dir_all};
use std::io::Read;
use std::os::fd::{BorrowedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::anyhow;
use camino::Utf8PathBuf;
use directories::exec_directories;
use nix::errno::Errno;
use nix::libc::{setresgid, setresuid};
use nix::sched::CloneFlags;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::instrument;
use tracing::{debug, error};
use tracing_log::log::info;
use uuid::Uuid;

mod subid;

pub mod identity;
pub mod mount;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Exec {
    pub cmd: String,
    pub args: Vec<String>,
}

impl Default for Exec {
    fn default() -> Self {
        Self {
            cmd: "true".to_string(),
            args: Default::default(),
        }
    }
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SpawnError {
    #[error("Failed to create user namespace: {0}")]
    CreateUserNamespace(#[source] CreateUserNamespaceError),

    #[error("Failed to spawn sandbox-run process: {0}")]
    ProcessSpawn(#[source] std::io::Error),

    #[error("Creating exec JSON dir: {0:?}")]
    CreateExecJson(#[source] std::io::Error),

    #[error("Writing exec JSON: {0}")]
    WriteExecJson(#[source] serde_json::Error),

    #[error("Failed to wait for process: {0}")]
    ProcessWait(#[from] std::io::Error),
}

#[instrument(ret)]
fn zaun_exe() -> String {
    #[cfg(not(any(test, feature = "testing")))]
    {
        let myself: String = std::env::args().next().unwrap();

        if myself.ends_with("/zaun") {
            myself
        } else {
            "zaun".to_string()
        }
    }

    #[cfg(any(test, feature = "testing"))]
    {
        use std::path::PathBuf;
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        debug!(
            "finding zaun executable in test mode. manifest_dir: {manifest_dir} current_dir: {:?}",
            std::env::current_dir()
        );
        PathBuf::from(manifest_dir)
            .parent()
            .unwrap()
            .join("target")
            .join("debug")
            .join("zaun")
            .canonicalize()
            .expect("canonicalize failed")
            .to_str()
            .unwrap()
            .to_owned()
    }
}

pub const EXEC_JSON_FILE_NAME: &str = "exec.json";

/// Implementation of `zaun spawn`.
/// Spans a `zaun exec` command in a new user namespace.
#[instrument]
pub fn spawn(exec_dir: &Path, exec: &Exec) -> Result<(), SpawnError> {
    let user_ns_fd = create_user_namespace().map_err(SpawnError::CreateUserNamespace)?;

    debug!("user_ns_fd: {user_ns_fd}");

    create_dir_all(exec_dir).map_err(SpawnError::CreateExecJson)?;
    let exe_json_path = exec_dir.join(EXEC_JSON_FILE_NAME);

    let exe_json_file = File::create_new(&exe_json_path).map_err(SpawnError::CreateExecJson)?;
    serde_json::to_writer_pretty(exe_json_file, &exec).map_err(SpawnError::WriteExecJson)?;

    let zaun_exe = zaun_exe();
    info!("zaun_exe: {zaun_exe}");
    let mut command = Command::new(zaun_exe);
    let command = command
        .arg("exec")
        .arg("--")
        .arg(exec_dir)
        .env_clear()
        .env("USER", "root")
        .env("TERM", "xterm-256color")
        .env("HOME", "/root")
        .env("PATH", "/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin")
        .env("RUST_LOG", "debug")
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    unsafe {
        command.pre_exec(move || {
            nix::sched::setns(
                BorrowedFd::borrow_raw(user_ns_fd),
                CloneFlags::CLONE_NEWUSER,
            )
            .map_err(|e| {
                std::io::Error::other(anyhow!("while setns to existing user namespace: {e}"))
            })?;

            // If we don't set this before exec, the capabilities are reset.

            let err = setresuid(0, 0, 0);
            if err != 0 {
                return Err(std::io::Error::last_os_error());
            }

            let err = setresgid(0, 0, 0);
            if err != 0 {
                return Err(std::io::Error::last_os_error());
            }

            Ok(())
        });
    }

    let mut child = command.spawn().map_err(SpawnError::ProcessSpawn)?;

    child.wait()?;

    Ok(())
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CreateUserNamespaceError {
    #[error("While setting up sub id ranges: {0}")]
    SubIdSetup(#[from] subid::Error),
    #[error("While spawning setup-user-ns: {0}")]
    SpawnSetupUserNs(#[source] std::io::Error),
    #[error("While reading setup sync byte from setup-user-ns: {0}")]
    ReadSetupSyncByte(#[source] std::io::Error),
    #[error("While opening user namespace file: {0:?}")]
    OpenUserNamespaceFile(#[source] Errno),
    #[error("While stopping setup-user-ns: {0:?}")]
    StoppingSetupUserNs(#[source] std::io::Error),
}

/// Create a new user namespace, sets up the subuid and subgid ranges
/// and returns the file descriptor to the new user namespace.
#[instrument]
fn create_user_namespace() -> Result<RawFd, CreateUserNamespaceError> {
    let id_map_reader = subid::IdMapMatcher::new_for_current_user()?;
    let uid_map = id_map_reader.get_matching_uid_map(1000)?;
    let gid_map = id_map_reader.get_matching_gid_map(1000)?;

    let mut command = Command::new(zaun_exe());
    let command = command
        .arg("setup-user-ns")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());

    let mut child = command
        .spawn()
        .map_err(CreateUserNamespaceError::SpawnSetupUserNs)?;

    let mut out = child.stdout.take().expect("stdout is not set");

    let buf = &mut [0; 1];
    out.read_exact(buf)
        .map_err(CreateUserNamespaceError::ReadSetupSyncByte)?;

    uid_map.call_newuidmap(child.id())?;
    gid_map.call_newgidmap(child.id())?;

    let stdin = child.stdin.take().expect("stdin is not set");

    // open its namespace FD
    let ns_fd = nix::fcntl::open(
        format!("/proc/{}/ns/user", child.id()).as_str(),
        nix::fcntl::OFlag::O_RDONLY | nix::fcntl::OFlag::O_CLOEXEC,
        nix::sys::stat::Mode::empty(),
    )
    .map_err(CreateUserNamespaceError::OpenUserNamespaceFile)?;

    std::mem::drop(stdin);

    child
        .kill()
        .map_err(CreateUserNamespaceError::StoppingSetupUserNs)?;
    Ok(ns_fd as RawFd)
}

/// Return a new UUID v7 (time-based + random) exec directory.
pub fn new_exec_dir() -> Utf8PathBuf {
    exec_directories().join(Uuid::now_v7().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spawn() {
        let exec_dir = tempfile::tempdir().unwrap();
        spawn(
            exec_dir.as_ref(),
            &Exec {
                ..Default::default()
            },
        )
        .unwrap();
    }
}
