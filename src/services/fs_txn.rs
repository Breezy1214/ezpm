use std::io;
use std::path::{Path, PathBuf};

use crate::services::atomic_file;

/// Records every file and folder a multi-step command touches, and puts them back as they were
/// if the command fails or is cancelled before `commit`.
#[derive(Default)]
pub struct FileTransaction {
    originals: Vec<(PathBuf, Option<Vec<u8>>)>,
    created_dirs: Vec<PathBuf>,
    committed: bool,
}

impl FileTransaction {
    pub fn new() -> Self {
        Self::default()
    }

    /// Remembers `path` as it is now, before something changes it.
    pub fn track(&mut self, path: &Path) -> io::Result<()> {
        if self.originals.iter().any(|(tracked, _)| tracked == path) {
            return Ok(());
        }
        let original = match std::fs::read(path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        self.originals.push((path.to_path_buf(), original));
        Ok(())
    }

    pub fn write(&mut self, path: &Path, contents: impl AsRef<[u8]>) -> io::Result<()> {
        self.track(path)?;
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            self.create_dir_all(parent)?;
        }
        atomic_file::write(path, contents)
    }

    /// Creates `path` and its missing parents; returns whether anything was created.
    pub fn create_dir_all(&mut self, path: &Path) -> io::Result<bool> {
        let missing: Vec<PathBuf> = path
            .ancestors()
            .take_while(|ancestor| !ancestor.as_os_str().is_empty() && !ancestor.exists())
            .map(Path::to_path_buf)
            .collect();
        if missing.is_empty() {
            return Ok(false);
        }
        std::fs::create_dir_all(path)?;
        self.created_dirs.extend(missing.into_iter().rev());
        Ok(true)
    }

    pub fn commit(mut self) {
        self.committed = true;
    }

    fn rollback(&mut self) {
        for (path, original) in self.originals.drain(..).rev() {
            let _ = match original {
                Some(bytes) => atomic_file::write(&path, bytes),
                None => match std::fs::remove_file(&path) {
                    Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                    other => other,
                },
            };
        }
        for dir in self.created_dirs.drain(..).rev() {
            let _ = std::fs::remove_dir(dir);
        }
    }
}

impl Drop for FileTransaction {
    fn drop(&mut self) {
        if !self.committed {
            self.rollback();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropping_without_commit_restores_files_and_removes_new_folders() {
        let dir = tempfile::TempDir::new().unwrap();
        let existing = dir.path().join("ezpm.toml");
        std::fs::write(&existing, "old").unwrap();
        let created = dir.path().join("src/client/init.luau");

        {
            let mut txn = FileTransaction::new();
            txn.write(&existing, "new").unwrap();
            txn.write(&created, "return {}").unwrap();
            assert_eq!(std::fs::read_to_string(&existing).unwrap(), "new");
        }

        assert_eq!(std::fs::read_to_string(&existing).unwrap(), "old");
        assert!(!dir.path().join("src").exists());
    }

    #[test]
    fn commit_keeps_changes() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("a/b.txt");
        let mut txn = FileTransaction::new();
        txn.write(&path, "kept").unwrap();
        txn.commit();
        assert_eq!(std::fs::read_to_string(path).unwrap(), "kept");
    }
}
