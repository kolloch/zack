use std::fmt::Display;

use camino::Utf8PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BuildConfigId(pub i32);

impl BuildConfigId {
    pub const DEFAULT_CONFIG_ID: BuildConfigId = BuildConfigId(1);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildConfig {
    pub id: BuildConfigId,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FileKind {
    Source,
    Built(BuildConfigId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileId(i32);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct File {
    pub id: FileId,
    build_config_id: Option<BuildConfigId>,
    pub rel_path: DbPathBuf,
    pub content_hash: Hash,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DbPathBuf(Utf8PathBuf);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Hash {
    internal: blake3::Hash,
}

impl Display for Hash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.internal)
    }
}

impl Ord for Hash {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.internal.as_bytes().cmp(other.internal.as_bytes())
    }
}
impl PartialOrd for Hash {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
