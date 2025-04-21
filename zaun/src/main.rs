use std::io::Read;
use std::os::fd::{BorrowedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::process::{Command, ExitStatus, Stdio};

use anyhow::anyhow;
use bpaf::Bpaf;
use nix::errno::Errno;
use nix::libc::{setresgid, setresuid};
use nix::sched::{unshare, CloneFlags};
use thiserror::Error;
use tracing::{debug, instrument};
use tracing::{info, error};

mod subid;

#[derive(Debug, Clone, Bpaf)]
#[bpaf(options, version)]
#[allow(dead_code)]
struct Opts {
    #[bpaf(external)]
    action: Action,
}

#[derive(Debug, Clone, Bpaf)]
enum Action {
    /// Setup a new user namespace and spawn
    /// a command via `zaun exec` in it.
    #[bpaf(command)]
    Spawn {
        #[bpaf(external)]
        exec: Exec,
    },
    /// Sets up the sandbox environment within
    /// the user namespace and runs the given command.
    #[bpaf(command)]
    Exec {
        #[bpaf(external)]
        exec: Exec,
    },
    /// Sets up a new user namespace with subid ranges.
    #[bpaf(command)]
    SetupUserNs {
    },
}

#[derive(Debug, Clone, Bpaf)]
struct Exec {
    #[bpaf(positional("CMD"))]
    cmd: String,
    #[bpaf(positional("ARGS"))]
    args: Vec<String>,
}


/// Implementation of `zaun setup-user-ns` called from [create_user_namespace].
#[instrument]
fn setup_user_ns() -> anyhow::Result<()> {
    // let id_map_reader = subid::IdMapReader::new_for_current_user()?;
    debug!("About to unshare...");

    unshare(CloneFlags::CLONE_NEWUSER)
        .map_err(|e| anyhow::anyhow!("unshare failed: {e}"))?;

    // Signal that we executed unshare.
    println!();

    info!("Set up finished.");

    // read one line from stdin so that we don't terminate immediately.
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;

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

    let myself: Vec<String> = std::env::args().collect();
    let mut command = Command::new(&myself[0]);
    let command = command
        .arg("setup-user-ns")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());

    debug!("Spawn command: {command:?}");
    let mut child = command
        .spawn()
        .map_err(CreateUserNamespaceError::SpawnSetupUserNs)?;
    
    let mut out = child.stdout
        .take()
        .expect("stdout is not set");

    let buf = &mut [0; 1];
    out.read_exact(buf).map_err(CreateUserNamespaceError::ReadSetupSyncByte)?;

    uid_map.call_newuidmap(child.id())?;
    gid_map.call_newgidmap(child.id())?;


    let stdin = child.stdin
        .take()
        .expect("stdin is not set");

    // open its namespace FD
    let ns_fd = nix::fcntl::open(
        format!("/proc/{}/ns/user", child.id()).as_str(),
        nix::fcntl::OFlag::O_RDONLY | nix::fcntl::OFlag::O_CLOEXEC,
        nix::sys::stat::Mode::empty(),
    ).map_err(CreateUserNamespaceError::OpenUserNamespaceFile)?;

    std::mem::drop(stdin);

    child.kill().map_err(CreateUserNamespaceError::StoppingSetupUserNs)?;
    Ok(ns_fd as RawFd)
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SpawnError {
    #[error("Failed to create user namespace: {0}")]
    CreateUserNamespace(#[source] CreateUserNamespaceError),

    #[error("Failed to spawn sandbox-run process: {0}")]
    ProcessSpawn(#[source] std::io::Error),

    #[error("Failed to wait for process: {0}")]
    ProcessWait(#[from] std::io::Error),
}

/// Implementation of `zaun spawn`.
/// Spans a `zaun exec` command in a new user namespace.
#[instrument]
fn spawn(exec: &Exec) -> Result<(), SpawnError> {
    let user_ns_fd = create_user_namespace().map_err(SpawnError::CreateUserNamespace)?;

    let myself: Vec<String> = std::env::args().collect();
    let mut command = Command::new(&myself[0]);
    let command = command
        .arg("exec")
        .arg("--")
        .arg(&exec.cmd)
        .args(&exec.args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    unsafe {
        command
            .pre_exec(move || {
                nix::sched::setns(BorrowedFd::borrow_raw( user_ns_fd), CloneFlags::CLONE_NEWUSER)
                    .map_err(|e| std::io::Error::other(anyhow!("while setns to existing user namespace: {e}")))?;

                let err = setresuid(0, 0, 0);
                if err != 0 {
                    return Err(std::io::Error::last_os_error());
                }

                let err= setresgid(0, 0, 0);
                if err != 0 {
                    return Err(std::io::Error::last_os_error());
                }

                Ok(())
            });
    }

    let mut child = command
        .spawn()
        .map_err(SpawnError::ProcessSpawn)?;

    child.wait()?;

    Ok(())
}


#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ExecError {
    #[error("unsharing failed {0:?}: {1}")]
    Unshare(CloneFlags, #[source] Errno),
    #[error("Failed to spawn process: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("Failed to wait for process: {0}")]
    Wait(#[source] std::io::Error),
}

#[instrument]
fn exec_command(exec: &Exec) -> Result<ExitStatus, ExecError> {

    // FIXME: Change to the correct userid, groupid and capabilities.

    let euid = nix::unistd::geteuid().as_raw();
    let egid = nix::unistd::getegid().as_raw();
    debug!("euid: {euid} egid: {egid}");
    debug!("caps: {:?}", caps::all());

    let flags = CloneFlags::CLONE_NEWNS
        | CloneFlags::CLONE_NEWIPC
        | CloneFlags::CLONE_NEWNET
        | CloneFlags::CLONE_NEWPID
        | CloneFlags::CLONE_NEWUTS
        | CloneFlags::CLONE_NEWCGROUP;

    nix::sched::unshare(flags)
        .map_err(|e| ExecError::Unshare(flags, e))?;

    // FIXME: Setup various namespaces.

    let exit_status = Command::new(&exec.cmd)
        .args(&exec.args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn().map_err(ExecError::Spawn)?
        .wait().map_err(ExecError::Wait)?;

    Ok(exit_status)
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    #[error("While trying to spawn: {0}")]
    Spawn(#[from] SpawnError),
    #[error("While setting up new user namespace: {0}")]
    SetupUserNs(#[source] anyhow::Error),
    #[error("While trying to execute in sub process: {0}")]
    Exec(#[from] ExecError),
}

fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    
    let options = opts().fallback_to_usage().run();

    info!("{options:?}");

    match &options.action {
        Action::Spawn { exec } => spawn(exec)?,
        Action::Exec {
            exec,
        } => {
            let exit_status = exec_command(exec)?;
            if exit_status.success() {
                info!("Command executed successfully");
            } else {
                error!("Command failed with status: {exit_status}");
                std::process::exit(exit_status.code().unwrap_or(1));
            }
        },
        Action::SetupUserNs {} => setup_user_ns().map_err(Error::SetupUserNs)?,
    }

    Ok(())
}
