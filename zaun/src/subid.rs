//! Helpers for setting up sub id ranges in user namespaces.
//! Using the `newuidmap` and `newgidmap` commands.

use std::{io::BufRead, path::{Path, PathBuf}};

use nix::{errno::Errno, unistd::User};
use thiserror::Error;
use tracing::instrument;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum UserNameOrId {
    Name(String),
    Id(u32),
}

impl UserNameOrId {
    fn from_str(s: &str) -> Self {
        if let Ok(id) = s.parse::<u32>() {
            UserNameOrId::Id(id)
        } else {
            UserNameOrId::Name(s.to_string())
        }
    }
}

/// Identifies a user by id and an optional name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UserNameAndId {
    name: Option<String>,
    id: u32,
}

impl UserNameAndId {
    pub fn current_user() -> Result<UserNameAndId> {
        let my_uid = nix::unistd::getuid();
        let my_user = User::from_uid(my_uid).map_err(Error::GetUserInfo)?;
        let my_name = my_user.map(|u| u.name);
        Ok(UserNameAndId {
            name: my_name,
            id: my_uid.as_raw(),
        })
    }
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    #[error("While trying to get user info: {0:?}")]
    GetUserInfo(#[source] Errno),
    #[error("Failed to open file {0:?}: {1}")]
    IdMapFileOpen(PathBuf, #[source] std::io::Error),
    #[error("Error while reading from {0:?}: {1}")]
    IdMapFileRead(PathBuf, #[source] std::io::Error),
    #[error("Failed to parse id range from line in {file:?}: {line:?}")]
    ParseSubIdRange { file: PathBuf, line: String },
    #[error("Range found in {file:?} contains {count} < {allowed_count} ids.")]
    SubIdRangeTooSmall { file: PathBuf, count: u32, allowed_count: u32 },
    #[error("No subuid range found for user {user:?} in {file:?}")]
    NoMatchingSubIdRange { file: PathBuf, user: UserNameAndId},
    #[error("Spawn newid command failed {command} {args:?}: {err}")]
    SpawnNewIdMapCommand { command: String, args: Vec<String>, err: std::io::Error },
    #[error("Error while calling {command} {args:?}\nOutput: {output:?}")]
    NewIdMapCommand {
        command: String,
        args: Vec<String>,
        output: std::process::Output,
    },
}

type Result<T> = std::result::Result<T, Error>;

/// Mockable file opener for testing.
pub trait FileOpener: std::fmt::Debug {
    fn open(&self, path: impl AsRef<Path>) -> Result<Box<dyn BufRead>>;
}

/// Default file opener that uses the standard library to open files.
#[derive(Debug)]
pub struct StdFileOpener;

impl FileOpener for StdFileOpener {
    fn open(&self, path: impl AsRef<Path>) -> Result<Box<dyn BufRead>> {
        let file = std::fs::File::open(path.as_ref()).map_err(|e| Error::IdMapFileOpen(path.as_ref().to_path_buf(), e))?;
        let reader = std::io::BufReader::new(file);
        Ok(Box::new(reader))
    }
}

/// Represents a UID/GID range mapping between the inner and the parent user namespace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdRange {
    outside_id: u32,
    inside_id: u32,
    count: u32,
}

impl IdRange {
    #[instrument]
    pub fn call_newuidmap(&self, pid: u32) -> Result<()> {
        self.call_newidmap("newuidmap", pid)
    }

    #[instrument]
    pub fn call_newgidmap(&self, pid: u32) -> Result<()> {
        self.call_newidmap("newgidmap", pid)
    }

    #[instrument]
    fn call_newidmap(&self, command: &str, pid: u32) -> Result<()> {
        let args = vec![
            pid.to_string(),
            self.inside_id.to_string(),
            self.outside_id.to_string(),
            self.count.to_string(),
        ];
        let newuidmap = std::process::Command::new(command)
            .args(&args)
            .output().map_err(|err| Error::SpawnNewIdMapCommand { command: command.to_string(), args: args.clone(), err })?;
        if !newuidmap.status.success() {
            return Err(Error::NewIdMapCommand { command: command.to_string(), args, output: newuidmap })
        }
        Ok(())
    }
}

const SUBUID_FILE: &str = "/etc/subuid";
const SUBGID_FILE: &str = "/etc/subgid";

/// Finds matching id map ranges from system config files.
#[derive(Debug)]
pub struct IdMapMatcher<FO: FileOpener = StdFileOpener> {
    user: UserNameAndId,
    file_opener: FO,
}

impl IdMapMatcher {
    /// Creates a new IdMapMatcher for the current user.
    pub fn new_for_current_user() -> Result<Self> {
        Ok(IdMapMatcher {
            user: UserNameAndId::current_user()?,
            file_opener: StdFileOpener,
        })
    }
}

impl<FO: FileOpener> IdMapMatcher<FO> {
    /// Returns a matching UID map for the given count (= range size).
    #[instrument]
    pub fn get_matching_uid_map(&self, count: u32) -> Result<IdRange> {
        self.get_matching_id_range_start(SUBUID_FILE, count)
    }
    
    /// Returns a matching GID map for the given count (= range size).
    #[instrument]
    pub fn get_matching_gid_map(&self, count: u32) -> Result<IdRange> {
        self.get_matching_id_range_start(SUBGID_FILE, count)
    }

    #[instrument(fields(path = path.as_ref().to_str()))]
    fn get_matching_id_range_start(
        &self,
        path: impl AsRef<Path>,
        count: u32,
    ) -> Result<IdRange> {
        let id_ranges = self.parse_subid_file(path.as_ref())?;
        if let Some(allowed_subuid_range) = id_ranges.iter().find(|(user, _, _)| match user {
            UserNameOrId::Name(name) => Some(name) == self.user.name.as_ref(),
            UserNameOrId::Id(id) => self.user.id == *id,
        }) {
            let (_, start, allowed_count) = allowed_subuid_range;
    
            if *allowed_count < count {
                return Err(Error::SubIdRangeTooSmall { file: path.as_ref().to_path_buf(), count, allowed_count: *allowed_count })
            }
    
            Ok(IdRange { outside_id: *start, inside_id: 0, count })
        } else {
            Err(Error::NoMatchingSubIdRange { file: path.as_ref().to_path_buf(), user: self.user.clone() })
        }
    }

    fn parse_subid_file(&self, path: impl AsRef<Path>) -> Result<Vec<(UserNameOrId, u32, u32)>> {
        let reader = self.file_opener.open(path.as_ref())?;
        let mut result = Vec::new();
    
        for line in reader.lines() {
            let line = line.map_err(|e| Error::IdMapFileRead(path.as_ref().to_path_buf(), e))?;
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let parts: Vec<&str> = line.split(':').map(str::trim).collect();
            let parse_err = || Error::ParseSubIdRange { file: path.as_ref().to_path_buf(), line: line.to_string() };
            if parts.len() != 3 {
                return Err(parse_err());
            }

            let name_or_id = UserNameOrId::from_str(parts[0]);
            let start_id: u32 = parts[1]
                .parse()
                .map_err(|_| parse_err())?;
            let count: u32 = parts[2]
                .parse()
                .map_err(|_| parse_err())?;
            result.push((name_or_id, start_id, count));
        }
    
        Ok(result)
    }    
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct FakeFileOpener {
        subuid: String,
        subgid: String,
    }

    impl FileOpener for FakeFileOpener {
        fn open(&self, path: impl AsRef<Path>) -> Result<Box<dyn BufRead>> {
            let content = if path.as_ref() == Path::new(SUBUID_FILE) {
                self.subuid.clone()
            } else {
                self.subgid.clone()
            };
            let reader = std::io::Cursor::new(content);
            Ok(Box::new(reader))
        }
    }

    fn fake_id_map_reader(subuid: &str, subgid: &str) -> IdMapMatcher<FakeFileOpener> {
        IdMapMatcher {
            user: UserNameAndId {
                name: Some("testuser".to_string()),
                id: 1000,
            },
            file_opener: FakeFileOpener {
                subuid: subuid.to_string(),
                subgid: subgid.to_string(),
            },
        }
    }

    #[test]
    fn matching_user_name() {
        let reader = fake_id_map_reader("testuser:1000:1", "testuser:1000:1");
        let result = reader.get_matching_uid_map(1);
        assert!(result.is_ok());
        let uid_map = result.unwrap();
        assert_eq!(uid_map.inside_id, 0);
        assert_eq!(uid_map.outside_id, 1000);
        assert_eq!(uid_map.count, 1);
    }

    #[test]
    fn matching_user_id() {
        let reader = fake_id_map_reader("1000:1000:1", "1000:1000:1");
        let result = reader.get_matching_uid_map(1);
        assert!(result.is_ok());
        let uid_map = result.unwrap();
        assert_eq!(uid_map.inside_id, 0);
        assert_eq!(uid_map.outside_id, 1000);
        assert_eq!(uid_map.count, 1);
    }

    #[test]
    fn no_matching_user() {
        let reader = fake_id_map_reader("otheruser:1000:1", "otheruser:1000:1");
        let result = reader.get_matching_uid_map(1);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "No subuid range found for user UserNameAndId { name: Some(\"testuser\"), id: 1000 } in \"/etc/subuid\""
        );
    }

    #[test]
    fn other_users() {
        let reader = fake_id_map_reader(
            "otheruser:1000:1\n\
            testuser:2000:1", 
            "otheruser:1000:1",
        );

        let result = reader.get_matching_uid_map(1);
        assert!(result.is_ok());
        let uid_map = result.unwrap();
        assert_eq!(uid_map.inside_id, 0);
        assert_eq!(uid_map.outside_id, 2000);
        assert_eq!(uid_map.count, 1);
    }

    #[test]
    fn not_enough_uids() {
        let reader = fake_id_map_reader("testuser:1000:1", "testuser:1000:1");
        let result = reader.get_matching_uid_map(2);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "Range found in \"/etc/subuid\" contains 2 < 1 ids."
        );
    }
}