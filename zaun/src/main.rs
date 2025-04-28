use std::collections::{BTreeMap, BTreeSet};
use std::process::{Command, ExitStatus, Stdio};

use anyhow::{Context, anyhow};
use bpaf::Bpaf;
use camino::{Utf8Path, Utf8PathBuf};
use caps::errors::CapsError;
use caps::{CapSet, Capability};
use nix::errno::Errno;
use nix::libc::umount;
use nix::mount::{self, mount, umount2, MntFlags, MsFlags};
use nix::sched::{CloneFlags, unshare};
use nix::unistd::{gethostname, pivot_root};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::io::Read;
use sys_mount::{Mount, MountFlags};
use thiserror::Error;
use tracing::{debug, instrument};
use tracing::{error, info};
use zaun::identity::{Groups, NameAndId};
use zaun::{EXEC_JSON_FILE_NAME, new_exec_dir};

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
        #[bpaf(positional("EXEC_DIR"))]
        exec_dir: Utf8PathBuf,
    },
    /// Sets up a new user namespace with subid ranges.
    #[bpaf(command)]
    SetupUserNs {},
    /// Dumps information about the process environment,
    /// etc.
    #[bpaf(command)]
    Probe {},
}

#[derive(Debug, Clone, Bpaf)]
struct Exec {
    #[bpaf(positional("CMD"))]
    cmd: String,
    #[bpaf(positional("ARGS"))]
    args: Vec<String>,
}

impl From<Exec> for zaun::Exec {
    fn from(Exec { cmd, args }: Exec) -> Self {
        Self { cmd, args }
    }
}

/// Implementation of `zaun setup-user-ns` called from [create_user_namespace].
#[instrument]
fn setup_user_ns() -> anyhow::Result<()> {
    // let id_map_reader = subid::IdMapReader::new_for_current_user()?;
    debug!("About to unshare...");

    unshare(CloneFlags::CLONE_NEWUSER).map_err(|e| anyhow::anyhow!("unshare failed: {e}"))?;

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
    #[error("While creating temporary directory for root: {0}")]
    CreateTempRootDir(#[source] std::io::Error),
    #[error("While setting host name: {0:?}")]
    SetHostName(#[source] Errno),
    #[error("While mounting {0}: {1}")]
    Mount(String, #[source] std::io::Error),
    #[error("While mounting {0}: {1:?}")]
    NixMount(String, #[source] Errno),
    #[error("Invalid path for overlayfs layer: {0:?}")]
    InvalidOverlayfsLayerPath(Utf8PathBuf),
    #[error("Could not pivot root to {new_root}, binding old root to {old_root}: {errno:?}")]
    PivotRoot {
        new_root: Utf8PathBuf,
        old_root: Utf8PathBuf,
        #[source]
        errno: Errno,
    },
    #[error("{0}")]
    Unclassified(#[from] anyhow::Error),
}

fn valid_overlayfs_path(path: &Utf8Path) -> Result<(), ExecError> {
    static RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"/[a-z-A-Z0-9_/-]+").unwrap());
    RE.is_match(path.as_str())
        .then_some(())
        .ok_or_else(|| ExecError::InvalidOverlayfsLayerPath(path.to_owned()))
}

#[instrument]
fn exec_command(exec_dir: &Utf8Path) -> Result<ExitStatus, ExecError> {
    let exec_json = exec_dir.join(EXEC_JSON_FILE_NAME);

    let mut buffer = String::new();
    let mut file = std::fs::File::open(&exec_json).map_err(ExecError::ReadConfig)?;
    file.read_to_string(&mut buffer)
        .map_err(ExecError::ReadConfig)?;

    let exec: zaun::Exec = serde_json::from_str(&buffer).map_err(ExecError::ParseConfig)?;

    // FIXME: Change to the correct userid, groupid and capabilities.

    let euid = nix::unistd::geteuid().as_raw();
    let egid = nix::unistd::getegid().as_raw();
    debug!("euid: {euid} egid: {egid}");
    debug!("caps: {:?}", caps::read(None, CapSet::Effective));

    // CLONE_NEWPID needs another fork.
    let flags = CloneFlags::CLONE_NEWNS
        | CloneFlags::CLONE_NEWIPC
        | CloneFlags::CLONE_NEWNET
        | CloneFlags::CLONE_NEWUTS
        | CloneFlags::CLONE_NEWCGROUP;

    nix::sched::unshare(flags).map_err(|e| ExecError::Unshare(flags, e))?;

    nix::unistd::sethostname("zack").map_err(ExecError::SetHostName)?;

    fn create_dir(dir: Utf8PathBuf) -> anyhow::Result<Utf8PathBuf> {
        std::fs::create_dir(&dir).with_context(|| format!("while creating {dir:?}"))?;
        Ok(dir)
    }

    let output_dir = create_dir(exec_dir.join("out"))?;
    let work_dir = create_dir(exec_dir.join("work"))?;
    let new_combined_root_dir = create_dir(exec_dir.join("root"))?;

    let tmp_root_setup = Utf8PathBuf::from("/tmp");
    let root_sub_dir = |name: &str| {
        create_dir(tmp_root_setup.join(name))?;
        Ok::<_, anyhow::Error>(new_combined_root_dir.join(name))
    };

    Mount::builder()
        .fstype("tmpfs")
        .data("size=10M")
        .mount("tmpfs", &tmp_root_setup)
        .map_err(|e| ExecError::Mount("tmpfs".into(), e))?;

    let old_root = root_sub_dir("old_root")?;
    let new_proc = root_sub_dir("proc")?;
    let new_sys = root_sub_dir("sys")?;
    let new_dev = root_sub_dir("dev")?;

    let build_root = Utf8PathBuf::from("/build-root");

    valid_overlayfs_path(&build_root)?;
    valid_overlayfs_path(&tmp_root_setup)?;
    valid_overlayfs_path(&output_dir)?;
    valid_overlayfs_path(&work_dir)?;

    let data = format!(
        "userxattr,lowerdir={build_root}:{tmp_root_setup},upperdir={output_dir},workdir={work_dir}"
    );
    debug!("Mounting overlayfs with data: {data}");
    Mount::builder()
        .fstype("overlay")
        .data(&data)
        .mount("overlay", &new_combined_root_dir)
        .map_err(|e| ExecError::Mount(format!("overlayfs {data}"), e))?;

    Mount::builder()
        .flags(MountFlags::BIND | MountFlags::REC)
        .mount("/proc", new_proc)
        .map_err(|e| ExecError::Mount("proc".into(), e))?;

    Mount::builder()
        .flags(MountFlags::BIND | MountFlags::REC)
        .mount("/sys", new_sys)
        .map_err(|e| ExecError::Mount("sys".into(), e))?;

    Mount::builder()
        .flags(MountFlags::BIND | MountFlags::REC)
        .mount("/dev", new_dev)
        .map_err(|e| ExecError::Mount("dev".into(), e))?;

    pivot_root(
        new_combined_root_dir.as_str(),
        new_combined_root_dir.join("old_root").as_str(),
    )
    .map_err(|errno| ExecError::PivotRoot {
        new_root: new_combined_root_dir,
        old_root,
        errno,
    })?;

    // FIXME: remove old_root

    // https://github.com/containers/bubblewrap/blob/9ca3b05ec787acfb4b17bed37db5719fa777834f/bubblewrap.c#L3405
    mount(
        Some("/old_root"),
        "/old_root",
        None::<&str>,
        MsFlags::MS_SILENT | MsFlags::MS_REC | MsFlags::MS_PRIVATE,
        None::<&str>,
    )
    .map_err(|e| ExecError::NixMount("old_root private".into(), e))?;

    umount2(
        "/old_root",
        MntFlags::MNT_DETACH,
    )
    .map_err(|e| ExecError::NixMount("old_root umount".into(), e))?;

    // FIXME: Setup various namespaces.

    let exit_status = Command::new(&exec.cmd)
        .current_dir("/")
        .args(&exec.args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(ExecError::Spawn)?
        .wait()
        .map_err(ExecError::Wait)?;

    Ok(exit_status)
}

#[derive(Debug, Serialize, Deserialize)]
struct ProbeInfo {
    host_name: String,
    identity: Identity,
    env: BTreeMap<String, String>,
    working_directory: Utf8PathBuf,
    capabilities: Capabilities,
}

#[derive(Debug, Serialize, Deserialize)]
struct Identity {
    user: NameAndId,
    groups: Groups,
}

impl Identity {
    pub fn current() -> anyhow::Result<Identity> {
        Ok(Identity {
            user: NameAndId::current_user().context("getting current user info")?,
            groups: Groups::current()?,
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Capabilities {
    effective: BTreeSet<String>,
    extra_permitted: BTreeSet<String>,
    extra_in_bound: BTreeSet<String>,
    inheritable: BTreeSet<String>,
}

impl Capabilities {
    pub fn current() -> Result<Self, CapsError> {
        Self::of_tid(None)
    }

    pub fn of_tid(tid: Option<i32>) -> Result<Self, CapsError> {
        let effective = caps::read(tid, CapSet::Effective)?;
        let permitted = caps::read(tid, CapSet::Permitted)?;
        let bound = caps::read(tid, CapSet::Bounding)?;
        let inheritable = caps::read(tid, CapSet::Inheritable)?;

        fn string_set<'a>(set: impl IntoIterator<Item = &'a Capability>) -> BTreeSet<String> {
            set.into_iter().map(Capability::to_string).collect()
        }

        Ok(Self {
            effective: string_set(&effective),
            extra_permitted: string_set(permitted.difference(&effective)),
            extra_in_bound: string_set(bound.difference(&permitted)),
            inheritable: string_set(&inheritable),
        })
    }
}

fn probe() -> anyhow::Result<ProbeInfo> {
    let host_name = gethostname()?
        .into_string()
        .map_err(|e| anyhow!("Could not convert host name to UTF-8: {e:?}"))?;
    let identity = Identity::current()?;
    let env = std::env::vars().collect();
    let working_directory = std::env::current_dir().context("getting current working directory")?;
    let working_directory = Utf8PathBuf::from_path_buf(working_directory)
        .map_err(|e| anyhow!("current directory non-UTF8: {e:?}"))?;
    let capabilities = Capabilities::current()?;

    Ok(ProbeInfo {
        host_name,
        identity,
        env,
        working_directory,
        capabilities,
    })
}

fn print_probe() -> anyhow::Result<()> {
    let info = probe()?;
    serde_json::to_writer_pretty(std::io::stdout(), &info)?;
    Ok(())
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
    #[error("While probing the process environment: {0}")]
    Probe(#[from] anyhow::Error),
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
        Action::Spawn { exec } => {
            let exec_dir = new_exec_dir();
            zaun::spawn(exec_dir.as_std_path(), &exec.clone().into())?
        }
        Action::Exec { exec_dir } => {
            let exit_status = exec_command(exec_dir)?;
            if exit_status.success() {
                info!("Command executed successfully");
            } else {
                error!("Command failed with status: {exit_status}");
                std::process::exit(exit_status.code().unwrap_or(1));
            }
        }
        Action::SetupUserNs {} => setup_user_ns().map_err(Error::SetupUserNs)?,

        Action::Probe {} => print_probe().map_err(Error::Probe)?,
    }

    Ok(())
}
