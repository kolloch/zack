use std::{fs::Permissions, io::Read, str::FromStr};

use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key(blake3::Hash);

impl Key {
    pub fn rel_path(&self) -> Utf8PathBuf {
        let hex = self.0.to_hex();
        let mut path = Utf8PathBuf::from_str(&hex[0..2]).unwrap();
        let mut rest = &hex[2..];
        for _ in 1..3 {
            path = path.join(&rest[0..2]);
            rest = &rest[2..];
        }
        path = path.join(rest);
        path
    }
}

/// A content-addressed blob store.
pub trait Zwischen {
    fn store(&self, file: &Utf8Path) -> Result<Key>;
    fn retrieve(&self, key: &Key) -> Result<Utf8PathBuf>;
}

/// A FileSystem-based implementation of `Zwischen`.
#[derive(Debug, Clone)]
pub struct FileSystemZwischen {
    base_path: Utf8PathBuf,
}

impl FileSystemZwischen {
    pub fn new(base_path: Utf8PathBuf) -> Self {
        Self { base_path }
    }
}

impl Zwischen for FileSystemZwischen {
    fn store(&self, file: &Utf8Path) -> Result<Key> {
        let mut hasher = blake3::Hasher::new();

        let file_read = std::fs::OpenOptions::new()
            .read(true)
            .write(false)
            .create(false)
            .open(file)
            .with_context(|| format!("while opening {file:?}"))?;

        // make file read-only
        let metadata = file_read.metadata()?;
        let mut permissions = metadata.permissions();
        if !permissions.readonly() {
            permissions.set_readonly(true);
            std::fs::set_permissions(file, permissions)
                .with_context(|| format!("while setting permissions for {file:?}"))?;
        }

        // TODO: Make inode immutable
        // https://docs.rs/nix/latest/nix/sys/ioctl/

        let mut reader = std::io::BufReader::new(file_read);
        let mut buffer = [0; 8192];
        loop {
            let bytes_read = reader.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }
        std::mem::drop(reader);

        let key = Key(hasher.finalize());

        let target_path = self.base_path.join(key.rel_path());
        if !target_path.exists() {
            std::fs::create_dir_all(target_path.parent().unwrap())
                .with_context(|| format!("while creating directories for {target_path:?}"))?;
            std::fs::rename(file, &target_path)
                .with_context(|| format!("while copying {file:?} to {target_path:?}"))?;
        }

        Ok(key)
    }

    fn retrieve(&self, key: &Key) -> Result<Utf8PathBuf> {
        let target_path = self.base_path.join(key.rel_path());
        if !target_path.exists() {
            return Err(anyhow::anyhow!("File not found: {}", target_path));
        }
        Ok(target_path)
    }
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, anyhow};
    use std::{fs::File, io::Write, path::Path};
    use tempfile::{NamedTempFile, TempDir, tempfile};

    use super::*;

    #[test]
    fn key_rel_path() {
        let key = Key(blake3::Hash::from_hex(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .unwrap());
        let rel_path = key.rel_path();
        assert_eq!(
            rel_path,
            Utf8PathBuf::from_str(
                "01/23/45/6789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            )
            .unwrap()
        );
    }

    #[derive(Debug)]
    struct ZwischenContext {
        dir: TempDir,
        zwischen: FileSystemZwischen,
        temp_files: Vec<NamedTempFile>,
    }

    impl ZwischenContext {
        fn new() -> Result<Self> {
            let dir = tempfile::tempdir()?;
            let base_path = Utf8PathBuf::from_str(
                dir.path()
                    .to_str()
                    .ok_or_else(|| anyhow!("unexpected non-utf8 file"))?,
            )?;
            let zwischen = FileSystemZwischen::new(base_path);
            Ok(Self {
                dir,
                zwischen,
                temp_files: Vec::new(),
            })
        }

        fn add_temp_file(&mut self) -> Result<Utf8PathBuf> {
            self.temp_files.push(NamedTempFile::new()?);
            let temp_file: &NamedTempFile = self.temp_files.last().unwrap();
            Ok(Utf8PathBuf::from_path_buf(temp_file.path().to_path_buf())
                .map_err(|e| anyhow!("unexpected non-utf8 file: {e:?}"))?)
        }
    }

    fn write_content(file: impl AsRef<Path>, content: &[u8]) -> Result<()> {
        let mut file = File::create(file.as_ref())
            .with_context(|| format!("while creating file {:?}", file.as_ref()))?;
        file.write_all(content)?;
        Ok(())
    }

    #[test]
    fn store_and_retrieve_file() -> Result<()> {
        let mut context = ZwischenContext::new()?;
        const CONTENT: &[u8; 16] = b"Hello, Zwischen!";

        let test_file: Utf8PathBuf = context.add_temp_file()?;
        write_content(&test_file, CONTENT)?;

        let key = context.zwischen.store(&test_file)?;

        let retrieved_path = context.zwischen.retrieve(&key)?;

        write_content(&test_file, b"stuff")?;

        let mut retrieved_file = File::open(&retrieved_path)
            .with_context(|| format!("while opening retrieved file {:?}", retrieved_path))?;
        let mut retrieved_content = Vec::new();
        retrieved_file
            .read_to_end(&mut retrieved_content)
            .with_context(|| format!("while reading retrieved file {:?}", retrieved_path))?;

        assert_eq!(
            retrieved_content, CONTENT,
            "Content mismatch after retrieval"
        );

        Ok(())
    }
}
