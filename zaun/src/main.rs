use std::process::{Command, ExitStatus, Stdio};

use bpaf::Bpaf;
use nix::errno::Errno;
use nix::sched::{unshare, CloneFlags};
use thiserror::Error;
use tracing::{debug, instrument};
use tracing::{info, error};
use std::io::Read;

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

impl From<Exec> for zaun::Exec {
    fn from(Exec { cmd, args}: Exec) -> Self {
        Self { cmd, args }
    }
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
pub enum ExecError {
    #[error("unsharing failed {0:?}: {1}")]
    Unshare(CloneFlags, #[source] Errno),
    #[error("Failed to spawn process: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("Failed to wait for process: {0}")]
    Wait(#[source] std::io::Error),
    #[error("While reading config from stdin: {0}")]
    ReadConfig(#[source] std::io::Error),
    #[error("Wwhile reading config from stdin: {0}")]
    ParseConfig(#[source] serde_json::Error),
}

#[instrument]
fn exec_command() -> Result<ExitStatus, ExecError> {
    // Read zaun::Exec from stdin as json
    let mut buffer = String::new();
    std::io::stdin().read_to_string(&mut buffer).map_err(ExecError::ReadConfig)?;
    let exec: zaun::Exec = serde_json::from_str(&buffer).map_err(|e| {
        ExecError::ParseConfig(e)
    })?;

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
    Spawn(#[from] zaun::SpawnError),
    #[error("While setting up new user namespace: {0}")]
    SetupUserNs(#[source] anyhow::Error),
    #[error("While trying to execute in sub process: {0}")]
    Exec(#[from] ExecError),
}

fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_file(true)
        .with_line_number(true)
        .init();
    
    let options = opts().fallback_to_usage().run();

    info!("{options:?}");

    match &options.action {
        Action::Spawn { exec } => zaun::spawn(&exec.clone().into())?,
        Action::Exec { } => {
            let exit_status = exec_command()?;
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
