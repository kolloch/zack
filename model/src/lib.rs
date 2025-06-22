use std::collections::BTreeMap;

use blake3::Hash;
use serde::{Deserialize, Serialize};

use crate::hash::Hashable;

pub mod hash;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct Dir {
    entries: BTreeMap<String, DirEntry>,
    entry_hash: Hash,
}

impl Dir {
    pub fn from_entries(entries: BTreeMap<String, DirEntry>) -> Self {
        Dir {
            entry_hash: entries.hash(),
            entries,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct DirEntry {
    kind: DirEntryKind,
    content_hash: Hash,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum DirEntryKind {
    Dir,
    File { attributes: FileAttributes },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct FileAttributes {
    executable: bool,
    size: u64,
}

trait DirStore {
    fn store_dir(&self, dir: &Dir) -> Result<(), anyhow::Error>;
}
