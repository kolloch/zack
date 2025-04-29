use artifact::Artifact;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::fs;
use std::io;
use thiserror::Error;

pub mod artifact;

#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error occurred on path {path}: {source}")]
    Io {
        path: Utf8PathBuf,
        context: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("Failed to create directory: {path}")]
    CreateDir {
        path: Utf8PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("Failed to create hard link from {source_path} to {target_path}: {source}")]
    CreateHardLink {
        source_path: Utf8PathBuf,
        target_path: Utf8PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("Entry validation error: {source}")]
    EntryValidation {
        #[source]
        source: artifact::EntryError,
    },
    #[error("Expected {a:?} to be strictly before {b:?}.")]
    ArtifactOrder { a: Artifact, b: Artifact },
    #[error("{b:?} conflicts with {a:?}")]
    ArtifactConflict { a: Artifact, b: Artifact },
}

/// Make the sorted paths from `from` available in `to`.
/// Create missing directories automatically.
/// Go through directories in `from` recursively.
/// Use hard links for files and make files read-only.
pub fn provision<'a>(
    from: &Utf8Path,
    to: &Utf8Path,
    sorted_paths: impl Iterator<Item = &'a Artifact>,
) -> Result<(), Error> {
    let mut last_entry: Option<Cow<'a, Artifact>> = None;
    for entry in sorted_paths {
        let validated_entry = entry
            .validate()
            .map_err(|e| Error::EntryValidation { source: e })?;
        let relative_path = validated_entry.path();
        if let Some(last_entry) = last_entry {
            if last_entry.as_ref().cmp(validated_entry.as_ref()) != Ordering::Less {
                return Err(Error::ArtifactOrder {
                    a: last_entry.into_owned().to_owned(),
                    b: entry.to_owned(),
                });
            }

            if relative_path.starts_with(last_entry.path()) {
                return Err(Error::ArtifactConflict {
                    a: last_entry.into_owned().to_owned(),
                    b: entry.to_owned(),
                });
            }
        }

        last_entry = Some(validated_entry.clone());

        let source_path = from.join(relative_path);
        let target_path = to.join(relative_path);

        match &*validated_entry {
            Artifact::Directory(_) => {
                if !target_path.exists() {
                    fs::create_dir_all(&target_path).map_err(|e| Error::CreateDir {
                        path: target_path.clone(),
                        source: e,
                    })?;
                }
                // Recursively process directory contents
                for entry in fs::read_dir(&source_path).map_err(|e| Error::Io {
                    context: "while reading directory contents",
                    path: source_path.clone(),
                    source: e,
                })? {
                    let entry = entry.map_err(|e| Error::Io {
                        context: "resolve dir entry",
                        path: source_path.clone(),
                        source: e,
                    })?;
                    let entry_path: Utf8PathBuf =
                        entry.path().try_into().map_err(|e| Error::Io {
                            context: "utf8-conversion, FIXME, extra error",
                            path: Utf8PathBuf::from(entry.path().to_string_lossy().to_string()),
                            source: io::Error::new(io::ErrorKind::InvalidData, e),
                        })?;
                    let entry_relative = entry_path.strip_prefix(from).map_err(|e| Error::Io {
                        context: "when stripping prefix, FIXME",
                        path: entry_path.clone(),
                        source: io::Error::new(io::ErrorKind::Other, e),
                    })?;

                    provision(
                        from,
                        to,
                        [if entry_path.is_dir() {
                            Artifact::Directory(entry_relative.to_owned())
                        } else {
                            Artifact::File(entry_relative.to_owned())
                        }]
                        .iter(),
                    )?;
                }
            }
            Artifact::File(_) => {
                if let Some(parent) = target_path.parent() {
                    if !parent.exists() {
                        fs::create_dir_all(parent).map_err(|e| Error::CreateDir {
                            path: parent.to_path_buf(),
                            source: e,
                        })?;
                    }
                }
                fs::hard_link(&source_path, &target_path).map_err(|e| Error::CreateHardLink {
                    source_path: source_path.clone(),
                    target_path: target_path.clone(),
                    source: e,
                })?;
                let mut perms = fs::metadata(&target_path)
                    .map_err(|e| Error::Io {
                        context: "get permissions",
                        path: target_path.clone(),
                        source: e,
                    })?
                    .permissions();
                perms.set_readonly(true);
                fs::set_permissions(&target_path, perms).map_err(|e| Error::Io {
                    context: "change permissions",
                    path: target_path.clone(),
                    source: e,
                })?;
            }
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn test_provision_empty() -> anyhow::Result<()> {
        let source_dir = tempdir()?;
        let target_dir = tempdir()?;

        let source_path = Utf8PathBuf::from_path_buf(source_dir.path().to_path_buf()).unwrap();
        let target_path = Utf8PathBuf::from_path_buf(target_dir.path().to_path_buf()).unwrap();

        provision(&source_path, &target_path, [].iter())?;

        // Check that target directory is empty
        let entries = fs::read_dir(&target_path)?.collect::<Result<Vec<_>, _>>()?;
        assert!(entries.is_empty(), "Target directory should be empty");

        Ok(())
    }

    #[test]
    fn test_provision_one_file() -> Result<(), Box<dyn std::error::Error>> {
        let source_dir = tempdir()?;
        let target_dir = tempdir()?;

        let source_path = Utf8PathBuf::from_path_buf(source_dir.path().to_path_buf()).unwrap();
        let target_path = Utf8PathBuf::from_path_buf(target_dir.path().to_path_buf()).unwrap();

        let test_file_path = "test.txt";
        let source_file = source_path.join(test_file_path);

        // Create source file
        let mut file = File::create(&source_file)?;
        writeln!(file, "test content")?;

        provision(
            &source_path,
            &target_path,
            [Artifact::File(test_file_path.into())].iter(),
        )?;

        // Check file exists in target
        let target_file = target_path.join(test_file_path);
        assert!(target_file.exists());
        assert_eq!(fs::read_to_string(&target_file)?, "test content\n");
        assert!(fs::metadata(&target_file)?.permissions().readonly());

        Ok(())
    }

    #[test]
    fn test_provision_one_directory_with_file() -> Result<(), Box<dyn std::error::Error>> {
        let source_dir = tempdir()?;
        let target_dir = tempdir()?;

        let source_path = Utf8PathBuf::from_path_buf(source_dir.path().to_path_buf()).unwrap();
        let target_path = Utf8PathBuf::from_path_buf(target_dir.path().to_path_buf()).unwrap();

        // Create directory and file structure
        let dir_path = "subdir";
        fs::create_dir(source_path.join(dir_path))?;

        let file_path = "other/test.txt";
        fs::create_dir(source_path.join("other"))?;
        let mut file = File::create(source_path.join(file_path))?;
        writeln!(file, "directory test content")?;

        provision(
            &source_path,
            &target_path,
            [
                Artifact::File(file_path.into()),
                Artifact::Directory(dir_path.into()),
            ]
            .iter(),
        )?;

        // Verify directory was created
        assert!(target_path.join(dir_path).exists());
        assert!(target_path.join(dir_path).is_dir());

        // Verify file was created and is read-only
        let target_file = target_path.join(file_path);
        assert!(target_file.exists());
        assert!(fs::metadata(&target_file)?.permissions().readonly());
        assert_eq!(
            fs::read_to_string(&target_file)?,
            "directory test content\n"
        );

        Ok(())
    }

    #[test]
    fn test_provision_nested_file_with_missing_parent_dirs()
    -> Result<(), Box<dyn std::error::Error>> {
        let source_dir = tempdir()?;
        let target_dir = tempdir()?;

        let source_path = Utf8PathBuf::from_path_buf(source_dir.path().to_path_buf()).unwrap();
        let target_path = Utf8PathBuf::from_path_buf(target_dir.path().to_path_buf()).unwrap();

        // Create nested file with parent directories
        fs::create_dir_all(source_path.join("a/b/c"))?;
        let mut file = File::create(source_path.join("a/b/c/nested.txt"))?;
        writeln!(file, "nested content")?;

        provision(
            &source_path,
            &target_path,
            [Artifact::File("a/b/c/nested.txt".into())].iter(),
        )?;

        // Check that parent directories were created
        assert!(target_path.join("a/b/c").exists());
        assert!(target_path.join("a/b/c").is_dir());

        // Check file content and permissions
        let target_file = target_path.join("a/b/c/nested.txt");
        assert!(target_file.exists());
        assert_eq!(fs::read_to_string(&target_file)?, "nested content\n");
        assert!(fs::metadata(&target_file)?.permissions().readonly());

        Ok(())
    }

    #[test]
    fn test_provision_directory() -> Result<(), Box<dyn std::error::Error>> {
        let source_dir = tempdir()?;
        let target_dir = tempdir()?;

        let source_path = Utf8PathBuf::from_path_buf(source_dir.path().to_path_buf()).unwrap();
        let target_path = Utf8PathBuf::from_path_buf(target_dir.path().to_path_buf()).unwrap();

        // Create directory structure
        fs::create_dir_all(source_path.join("dir/subdir"))?;

        provision(
            &source_path,
            &target_path,
            [Artifact::Directory("dir".into())].iter(),
        )?;

        // Verify directory structure was created
        assert!(target_path.join("dir").exists());
        assert!(target_path.join("dir").is_dir());
        assert!(target_path.join("dir/subdir").exists());
        assert!(target_path.join("dir/subdir").is_dir());

        Ok(())
    }

    #[test]
    fn test_provision_multiple_files() -> Result<(), Box<dyn std::error::Error>> {
        let source_dir = tempdir()?;
        let target_dir = tempdir()?;

        let source_path = Utf8PathBuf::from_path_buf(source_dir.path().to_path_buf()).unwrap();
        let target_path = Utf8PathBuf::from_path_buf(target_dir.path().to_path_buf()).unwrap();

        // Create multiple files
        let mut file1 = File::create(source_path.join("file1.txt"))?;
        writeln!(file1, "content 1")?;
        let mut file2 = File::create(source_path.join("file2.txt"))?;
        writeln!(file2, "content 2")?;

        provision(
            &source_path,
            &target_path,
            [
                Artifact::File("file1.txt".into()),
                Artifact::File("file2.txt".into()),
            ]
            .iter(),
        )?;

        // Verify both files exist with correct content
        assert_eq!(
            fs::read_to_string(target_path.join("file1.txt"))?,
            "content 1\n"
        );
        assert_eq!(
            fs::read_to_string(target_path.join("file2.txt"))?,
            "content 2\n"
        );

        Ok(())
    }

    #[test]
    fn test_provision_artifact_order_error() -> Result<(), Box<dyn std::error::Error>> {
        let source_dir = tempdir()?;
        let target_dir = tempdir()?;

        let source_path = Utf8PathBuf::from_path_buf(source_dir.path().to_path_buf()).unwrap();
        let target_path = Utf8PathBuf::from_path_buf(target_dir.path().to_path_buf()).unwrap();

        // Create test files
        fs::create_dir_all(source_path.join("dir"))?;
        let mut file = File::create(source_path.join("dir/file.txt"))?;
        writeln!(file, "content")?;

        // Test with unsorted artifacts (wrong order)
        let result = provision(
            &source_path,
            &target_path,
            [
                Artifact::File("dir/file.txt".into()),
                Artifact::Directory("dir".into()),
            ]
            .iter(),
        );

        assert!(result.is_err());
        match result {
            Err(Error::ArtifactOrder { a, b }) => {
                assert_eq!(a, Artifact::File("dir/file.txt".into()));
                assert_eq!(b, Artifact::Directory("dir".into()));
            }
            _ => panic!("Expected ArtifactOrder error"),
        }

        Ok(())
    }

    #[test]
    fn test_provision_conflict_error() -> Result<(), Box<dyn std::error::Error>> {
        let source_dir = tempdir()?;
        let target_dir = tempdir()?;

        let source_path = Utf8PathBuf::from_path_buf(source_dir.path().to_path_buf()).unwrap();
        let target_path = Utf8PathBuf::from_path_buf(target_dir.path().to_path_buf()).unwrap();

        // Create test files and directories
        fs::create_dir_all(source_path.join("dir/subdir"))?;
        let mut file = File::create(source_path.join("dir/file.txt"))?;
        writeln!(file, "content")?;

        // Test conflict: file inside a directory path
        let result = provision(
            &source_path,
            &target_path,
            [
                Artifact::Directory("dir".into()),
                Artifact::File("dir/file.txt".into()),
            ]
            .iter(),
        );

        assert!(result.is_err());
        match result {
            Err(Error::ArtifactConflict { a, b }) => {
                assert_eq!(a, Artifact::Directory("dir".into()));
                assert_eq!(b, Artifact::File("dir/file.txt".into()));
            }
            _ => panic!("Expected ArtifactConflict error instead of {result:?}"),
        }

        Ok(())
    }

    #[test]
    fn test_provision_existing_target_directory() -> Result<(), Box<dyn std::error::Error>> {
        let source_dir = tempdir()?;
        let target_dir = tempdir()?;

        let source_path = Utf8PathBuf::from_path_buf(source_dir.path().to_path_buf()).unwrap();
        let target_path = Utf8PathBuf::from_path_buf(target_dir.path().to_path_buf()).unwrap();

        // Create directory in source
        fs::create_dir_all(source_path.join("existing_dir"))?;

        // Create the same directory in target
        fs::create_dir_all(target_path.join("existing_dir"))?;

        // Provision should work with existing directories
        provision(
            &source_path,
            &target_path,
            [Artifact::Directory("existing_dir".into())].iter(),
        )?;

        assert!(target_path.join("existing_dir").exists());
        assert!(target_path.join("existing_dir").is_dir());

        Ok(())
    }
}
