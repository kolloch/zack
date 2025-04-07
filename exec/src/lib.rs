#![allow(unused)]

mod sandbox;

use blake3::Hash;
use camino::{Utf8Path, Utf8PathBuf};
use nix::NixPath;
use nix::errno::Errno;
use nix::mount::MsFlags;
use nix::sched::CloneFlags;
use nix::sys::signal::Signal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ffi::CString;
use std::path::PathBuf;
use tempfile::TempDir;
use thiserror::Error;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Action {
    exec: Exec,
    input_files: BTreeMap<Utf8PathBuf, ExistingFile>,
    output_files: BTreeMap<Utf8PathBuf, OutputFile>,
}

#[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Exec {
    command: Vec<String>,
    env: BTreeMap<String, EnvValue>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct EnvValue {
    value: String,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct OutputFile {}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct ExistingFile {
    hash: Hash,
}

#[derive(Debug)]
struct ActionResult {
    temp_dir: TempDir,
    output_dir: Utf8PathBuf,
}

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum ExecError {
    #[error("Could not create temporary directory: {0:?}")]
    CouldNotCreateTempDir(std::io::Error),
    #[error("Could not create subdirectory {0:?}: {1:?}")]
    CouldNotCreateSubDir(Utf8PathBuf, std::io::Error),
    #[error("Not a UTF-8 path: {0:?}")]
    NotUTF8Path(PathBuf),
    #[error("Error during sandbox clone: {0:?}")]
    SandboxClone(Errno),
    #[error("Error during sandbox unshare {1:?}: {0:?}")]
    SandboxUnshare(CloneFlags, Errno),
    #[error(
        "Error during sandbox mount -t {fstype:?} -o {data:?} {flags:?} {source:?} {target:?}: {err:?}"
    )]
    SandboxMount {
        err: Errno,
        r#source: Option<PathBuf>,
        target: PathBuf,
        fstype: Option<String>,
        flags: MsFlags,
        data: Option<String>,
    },
    #[error("Error during sandbox chdir to {0:?}: {1:?}")]
    SandboxChdir(PathBuf, Errno),
    #[error("Error waiting for child process: {0:?}")]
    SandboxWaitPid(Errno),
    #[error("Command was killed by signal: {0:?}")]
    SandboxKilled(Signal),
    #[error("Unexpected IO error: {0:?}")]
    UnexpectedIo(#[from] std::io::Error),
}

type Result<T> = std::result::Result<T, ExecError>;

impl Action {
    fn exec(&self, input_files_root: &Utf8Path) -> Result<ActionResult> {
        let temp_dir = tempfile::tempdir().map_err(ExecError::CouldNotCreateTempDir)?;
        let exec_dir = Utf8PathBuf::from_path_buf(temp_dir.as_ref().to_path_buf())
            .map_err(ExecError::NotUTF8Path)?;
        let input_dir = exec_dir.join("in");
        std::fs::create_dir_all(&input_dir)
            .map_err(|e| ExecError::CouldNotCreateSubDir(input_dir, e))?;
        let output_dir = exec_dir.join("out");
        std::fs::create_dir_all(&output_dir)
            .map_err(|e| ExecError::CouldNotCreateSubDir(output_dir.clone(), e))?;

        // copy inputs to input_dir
        // layer /build-root (ro), input_dir (ro), output_dir (ro)
        // pivot root
        // execute command

        todo!("implement");

        Ok(ActionResult {
            temp_dir,
            output_dir,
        })
    }
}
