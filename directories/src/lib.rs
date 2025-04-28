use camino::{Utf8Path, Utf8PathBuf};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use thiserror::Error;

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error(
        "No ZACK_WORKSPACE.star file found in any parent directory of '{current_dir:?}'.\n\
         Create one in this directory with:\n\
         \n\
         \t{exe} init"
    )]
    WorkspaceRootNotFound {
        exe: PathBuf,
        current_dir: Utf8PathBuf,
    },
    #[error("Not an UTF-8 compatible path: {0}")]
    NoUtf8(PathBuf),
    #[error("Unexpected IO error: {err:?}")]
    Io {
        #[from]
        err: std::io::Error,
    },
}

pub fn workspace_dir() -> &'static Utf8Path {
    paths().workspace.as_path()
}

pub fn target_dir() -> &'static Utf8Path {
    paths().target.as_path()
}

pub fn rules_dir() -> &'static Utf8Path {
    paths().rules.as_path()
}

pub fn out_dir() -> &'static Utf8Path {
    paths().out.as_path()
}

pub fn exec_directories() -> &'static Utf8Path {
    paths().exec.as_path()
}

#[derive(Debug)]
struct WorkspacePaths {
    workspace: Utf8PathBuf,
    target: Utf8PathBuf,
    rules: Utf8PathBuf,
    out: Utf8PathBuf,
    exec: Utf8PathBuf,
}

fn paths() -> &'static WorkspacePaths {
    static WORKSPACE_ROOT: OnceLock<WorkspacePaths> = OnceLock::new();

    WORKSPACE_ROOT.get_or_init(|| {
        let root = detect_workspace();
        if let Err(err) = root {
            eprintln!("{}", err);
            std::process::exit(1);
        }
        let root = Utf8PathBuf::from_path_buf(root.unwrap())
            .map_err(Error::NoUtf8)
            .unwrap();

        let target = root.join("target").join("zack");
        let rules = target.join("rules");
        let out = target.join("out");
        let exec = target.join("exec");
        WorkspacePaths {
            workspace: root,
            target,
            rules,
            out,
            exec,
        }
    })
}

/// Detects the workspace root directory by looking for a ZACK_WORKSPACE.star file
/// starting from the current directory and moving up through parent directories.
fn detect_workspace() -> Result<PathBuf, Error> {
    let current_dir = std::env::current_dir()?;
    let current_path = current_dir.as_path().canonicalize()?;
    let mut current_path: &Path = &current_path;

    loop {
        let workspace_file = current_path.join("ZACK_WORKSPACE.star");
        if workspace_file.exists() {
            return Ok(current_path.to_path_buf());
        }

        // Try to move up to parent directory
        match current_path.parent() {
            Some(parent) => current_path = parent,
            None => {
                return Err(Error::WorkspaceRootNotFound {
                    exe: std::env::current_exe()?,
                    current_dir: Utf8PathBuf::from_path_buf(current_dir).map_err(Error::NoUtf8)?,
                });
            } // Reached root directory
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::fs::File;
    use tempfile::TempDir;

    #[test]
    fn test_detect_workspace() -> anyhow::Result<()> {
        let temp_dir = TempDir::new()?;
        let workspace_root = temp_dir.path().canonicalize()?;
        let sub_dir = workspace_root.join("sub").join("deep");
        fs::create_dir_all(&sub_dir)?;

        File::create(workspace_root.join("ZACK_WORKSPACE.star"))?;

        std::env::set_current_dir(&sub_dir)?;

        let detected = detect_workspace()?;
        assert_eq!(detected, workspace_root);
        Ok(())
    }

    #[test]
    fn test_no_workspace() -> anyhow::Result<()> {
        // Create a temporary directory with no workspace file
        let temp_dir = TempDir::new()?;
        std::env::set_current_dir(temp_dir.path())?;
        let current_dir = std::env::current_dir()?;

        // Test detection
        let detected = detect_workspace();
        if let Err(Error::WorkspaceRootNotFound {
            current_dir: reported_current_dir,
            ..
        }) = detected
        {
            assert_eq!(current_dir, reported_current_dir);
        } else {
            panic!("error expected but got {detected:?} instead.")
        }
        Ok(())
    }
}
