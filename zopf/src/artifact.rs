use camino::Utf8Path;
use camino::Utf8PathBuf;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::borrow::Cow;
use std::fmt;
use thiserror::Error;

/// Relative directory or file artifact paths.
#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub enum Artifact {
    /// A directory with all its files and sub directories.
    Directory(Utf8PathBuf),
    /// A single file.
    File(Utf8PathBuf),
}

impl Artifact {
    pub fn path(&self) -> &Utf8Path {
        match self {
            Artifact::Directory(path) => path,
            Artifact::File(path) => path,
        }
    }
}

impl Ord for Artifact {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.path().cmp(other.path())
    }
}

impl PartialOrd for Artifact {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Error type for Entry-related operations.
#[non_exhaustive]
#[derive(Error, Debug)]
pub enum EntryError {
    /// Error when a path is not relative.
    #[error("Path must be relative: {0}")]
    NotRelative(Utf8PathBuf),
    /// Error when a path contains parent directory references.
    #[error("Path must not contain parent directory references: {0}")]
    ContainsParentComponents(Utf8PathBuf),

    /// IO error that occurred during entry operations.
    #[error("IO error on {0}: {1}")]
    Io(Utf8PathBuf, #[source] std::io::Error),
}

impl Artifact {
    /// Creates a new directory entry from a path.
    ///
    /// The path must be relative.
    pub fn new_dir<P: Into<Utf8PathBuf>>(path: P) -> Result<Self, EntryError> {
        let path = path.into();
        if path.is_absolute() {
            return Err(EntryError::NotRelative(path));
        }
        Ok(Artifact::Directory(path))
    }

    /// Creates a new file entry from a path.
    ///
    /// The path must be relative.
    pub fn new_file<P: Into<Utf8PathBuf>>(path: P) -> Result<Self, EntryError> {
        let path = path.into();
        if path.is_absolute() {
            return Err(EntryError::NotRelative(path));
        }
        Ok(Artifact::File(path))
    }

    /// Validates an entry, ensuring paths are relative and normalized.
    ///
    /// Returns a Cow containing either a reference to the original entry if it's valid
    /// or a new entry with necessary corrections.
    pub fn validate(&self) -> Result<Cow<'_, Artifact>, EntryError> {
        let path = match self {
            Artifact::Directory(path) => path,
            Artifact::File(path) => path,
        };

        if path.is_absolute() {
            return Err(EntryError::NotRelative(path.clone()));
        }

        if path
            .components()
            .any(|c| c == camino::Utf8Component::ParentDir)
        {
            return Err(EntryError::ContainsParentComponents(path.clone()));
        }

        // Normalize path by removing redundant segments, etc.
        let normalized_path = Self::normalize_path(path);

        if normalized_path.as_str() == path.as_str() {
            // No changes needed, return reference to original
            Ok(Cow::Borrowed(self))
        } else {
            // Create a new entry with the normalized path
            let normalized_entry = match self {
                Artifact::Directory(_) => Artifact::Directory(normalized_path),
                Artifact::File(_) => Artifact::File(normalized_path),
            };
            Ok(Cow::Owned(normalized_entry))
        }
    }

    /// Normalize a path by cleaning up redundant separators and segments.
    fn normalize_path(path: &Utf8PathBuf) -> Utf8PathBuf {
        let mut normalized = Utf8PathBuf::new();
        normalized.extend(
            path.components()
                .filter(|c| c != &camino::Utf8Component::CurDir),
        );
        normalized
    }
}

impl Serialize for Artifact {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Artifact::Directory(path) => {
                let path_str = ensure_ends_with_slash(path);
                serializer.serialize_str(&path_str)
            }
            Artifact::File(path) => {
                let path_str = ensure_no_trailing_slash(path);
                serializer.serialize_str(&path_str)
            }
        }
    }
}

impl<'de> Deserialize<'de> for Artifact {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct EntryVisitor;

        impl serde::de::Visitor<'_> for EntryVisitor {
            type Value = Artifact;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string representing a file or directory path")
            }

            fn visit_str<E>(self, value: &str) -> Result<Artifact, E>
            where
                E: serde::de::Error,
            {
                if value.ends_with('/') {
                    let path = Utf8PathBuf::from(value);
                    Ok(Artifact::Directory(path))
                } else {
                    let path = Utf8PathBuf::from(value);
                    Ok(Artifact::File(path))
                }
            }
        }

        deserializer.deserialize_str(EntryVisitor)
    }
}

fn ensure_ends_with_slash(path: &Utf8Path) -> String {
    let path_str = path.as_str();
    if path_str.ends_with('/') {
        path_str.to_string()
    } else {
        format!("{}/", path_str)
    }
}

fn ensure_no_trailing_slash(path: &Utf8Path) -> String {
    let path_str = path.as_str();
    if path_str.ends_with('/') {
        path_str[0..path_str.len() - 1].to_string()
    } else {
        path_str.to_string()
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_artifact_validation_success() {
        // Valid entries should return borrowed references
        let dir = Artifact::new_dir("path/to/dir").unwrap();
        let file = Artifact::new_file("path/to/file.txt").unwrap();

        assert!(matches!(dir.validate().unwrap(), Cow::Borrowed(_)));
        assert!(matches!(file.validate().unwrap(), Cow::Borrowed(_)));
    }

    #[test]
    fn test_artifact_validation_normalize_current_dir() {
        // Entry with current dir components should be normalized
        let dir = Artifact::Directory(Utf8PathBuf::from("path/./to/dir"));
        let file = Artifact::File(Utf8PathBuf::from("path/./to/file.txt"));

        let validated_dir = dir.validate().unwrap();
        let validated_file = file.validate().unwrap();

        println!("stuff {validated_dir:?}");
        assert!(matches!(validated_dir, Cow::Owned(_)));
        assert!(matches!(validated_file, Cow::Owned(_)));

        match validated_dir.into_owned() {
            Artifact::Directory(path) => assert_eq!(path, Utf8PathBuf::from("path/to/dir")),
            _ => panic!("Expected directory entry"),
        }

        match validated_file.into_owned() {
            Artifact::File(path) => assert_eq!(path, Utf8PathBuf::from("path/to/file.txt")),
            _ => panic!("Expected file entry"),
        }
    }

    #[test]
    fn test_artifact_validation_parent_components_error() {
        // Entries with parent directory references should fail validation
        let dir = Artifact::Directory(Utf8PathBuf::from("path/../to/dir"));
        let file = Artifact::File(Utf8PathBuf::from("path/to/../file.txt"));

        assert!(matches!(
            dir.validate(),
            Err(EntryError::ContainsParentComponents(_))
        ));
        assert!(matches!(
            file.validate(),
            Err(EntryError::ContainsParentComponents(_))
        ));
    }

    #[test]
    fn test_artifact_validation_absolute_path_error() {
        // Create entries directly to bypass the new_* checks
        let dir = Artifact::Directory(Utf8PathBuf::from("/absolute/path"));
        let file = Artifact::File(Utf8PathBuf::from("/absolute/file.txt"));

        assert!(matches!(dir.validate(), Err(EntryError::NotRelative(_))));
        assert!(matches!(file.validate(), Err(EntryError::NotRelative(_))));
    }

    #[test]
    fn test_normalize_path() {
        let path = Utf8PathBuf::from("a/./b/./c");
        let normalized = Artifact::normalize_path(&path);
        assert_eq!(normalized, Utf8PathBuf::from("a/b/c"));

        let path = Utf8PathBuf::from("./a/b/c/.");
        let normalized = Artifact::normalize_path(&path);
        assert_eq!(normalized, Utf8PathBuf::from("a/b/c"));
    }

    #[test]
    fn test_new_dir() {
        let dir = Artifact::new_dir("path/to/dir").unwrap();
        assert!(matches!(dir, Artifact::Directory(_)));
        if let Artifact::Directory(path) = dir {
            assert_eq!(path, Utf8PathBuf::from("path/to/dir"));
        }
    }

    #[test]
    fn test_new_file() {
        let file = Artifact::new_file("path/to/file.txt").unwrap();
        assert!(matches!(file, Artifact::File(_)));
        if let Artifact::File(path) = file {
            assert_eq!(path, Utf8PathBuf::from("path/to/file.txt"));
        }
    }

    #[test]
    fn test_absolute_path_errors() {
        assert!(matches!(
            Artifact::new_dir("/absolute/path"),
            Err(EntryError::NotRelative(_))
        ));
        assert!(matches!(
            Artifact::new_file("/absolute/path/file.txt"),
            Err(EntryError::NotRelative(_))
        ));
    }

    #[test]
    fn test_serialize_directory() {
        let dir = Artifact::new_dir("path/to/dir").unwrap();
        let serialized = serde_json::to_string(&dir).unwrap();
        assert_eq!(serialized, "\"path/to/dir/\"");
    }

    #[test]
    fn test_serialize_file() {
        let file = Artifact::new_file("path/to/file.txt").unwrap();
        let serialized = serde_json::to_string(&file).unwrap();
        assert_eq!(serialized, "\"path/to/file.txt\"");
    }

    #[test]
    fn test_deserialize_directory() {
        let dir_json = "\"path/to/dir/\"";
        let entry: Artifact = serde_json::from_str(dir_json).unwrap();
        assert!(matches!(entry, Artifact::Directory(_)));
        if let Artifact::Directory(path) = entry {
            assert_eq!(path, Utf8PathBuf::from("path/to/dir/"));
        }
    }

    #[test]
    fn test_deserialize_file() {
        let file_json = "\"path/to/file.txt\"";
        let entry: Artifact = serde_json::from_str(file_json).unwrap();
        assert!(matches!(entry, Artifact::File(_)));
        if let Artifact::File(path) = entry {
            assert_eq!(path, Utf8PathBuf::from("path/to/file.txt"));
        }
    }

    #[test]
    fn test_roundtrip_serialization() {
        let dir = Artifact::new_dir("path/to/dir").unwrap();
        let file = Artifact::new_file("path/to/file.txt").unwrap();

        let dir_json = serde_json::to_string(&dir).unwrap();
        let file_json = serde_json::to_string(&file).unwrap();

        let dir_deserialized: Artifact = serde_json::from_str(&dir_json).unwrap();
        let file_deserialized: Artifact = serde_json::from_str(&file_json).unwrap();

        assert_eq!(dir_deserialized, dir);
        assert_eq!(file_deserialized, file);
    }

    #[test]
    fn test_artifact_ordering() {
        // Test that directories and files are ordered by path
        let artifacts = vec![
            Artifact::new_file("c.txt").unwrap(),
            Artifact::new_dir("b").unwrap(),
            Artifact::new_file("a.txt").unwrap(),
            Artifact::new_dir("d").unwrap(),
        ];

        let mut sorted = artifacts.clone();
        sorted.sort();

        assert_eq!(sorted[0], Artifact::new_file("a.txt").unwrap());
        assert_eq!(sorted[1], Artifact::new_dir("b").unwrap());
        assert_eq!(sorted[2], Artifact::new_file("c.txt").unwrap());
        assert_eq!(sorted[3], Artifact::new_dir("d").unwrap());
    }

    #[test]
    fn test_artifact_comparing_dirs_and_files() {
        // Test comparison between directories and files with the same path
        let dir = Artifact::new_dir("path").unwrap();
        let file = Artifact::new_file("path").unwrap();

        // They should be considered equal for sorting purposes since they have the same path
        assert_eq!(dir.cmp(&file), std::cmp::Ordering::Equal);
        assert_eq!(file.cmp(&dir), std::cmp::Ordering::Equal);
    }

    #[test]
    fn test_artifact_nested_path_sorting() {
        // Test sorting with nested paths
        let mut artifacts = vec![
            Artifact::new_file("a/z.txt").unwrap(),
            Artifact::new_dir("a/b/c").unwrap(),
            Artifact::new_file("a/b/d.txt").unwrap(),
            Artifact::new_dir("a").unwrap(),
        ];

        artifacts.sort();

        assert_eq!(artifacts[0], Artifact::new_dir("a").unwrap());
        assert_eq!(artifacts[1], Artifact::new_dir("a/b/c").unwrap());
        assert_eq!(artifacts[2], Artifact::new_file("a/b/d.txt").unwrap());
        assert_eq!(artifacts[3], Artifact::new_file("a/z.txt").unwrap());
    }
}
