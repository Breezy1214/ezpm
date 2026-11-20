use std::path::{Component, Path, PathBuf};

/// The files or folders a command was asked to report on. Empty means everything.
pub struct PathFilter {
    roots: Vec<PathBuf>,
}

impl PathFilter {
    pub fn new(project_dir: &Path, paths: &[String]) -> Self {
        Self {
            roots: paths
                .iter()
                .map(|path| absolute(project_dir, Path::new(path)))
                .collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }

    /// Whether `path` (absolute, or relative to `project_dir`) is one of the paths or inside one.
    pub fn matches(&self, project_dir: &Path, path: &Path) -> bool {
        if self.roots.is_empty() {
            return true;
        }
        let path = absolute(project_dir, path);
        self.roots.iter().any(|root| path.starts_with(root))
    }
}

/// `path` as an absolute, lexically normalized path, resolving symlinks when it exists.
pub fn absolute(project_dir: &Path, path: &Path) -> PathBuf {
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        project_dir.join(path)
    };
    dunce::canonicalize(&joined).unwrap_or_else(|_| normalize(&joined))
}

fn normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other),
        }
    }
    normalized
}

/// `path` relative to `project_dir` with `/` separators, for messages and JSON.
pub fn display(project_dir: &Path, path: &Path) -> String {
    let project_dir = absolute(project_dir, Path::new("."));
    let path = absolute(&project_dir, path);
    path.strip_prefix(&project_dir)
        .unwrap_or(&path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_files_and_folders_relative_to_the_project() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("src/client")).unwrap();
        std::fs::write(dir.path().join("src/client/a.luau"), "").unwrap();
        std::fs::write(dir.path().join("src/b.luau"), "").unwrap();

        let filter = PathFilter::new(dir.path(), &["./src/client/".to_string()]);
        assert!(filter.matches(dir.path(), Path::new("src/client/a.luau")));
        assert!(!filter.matches(dir.path(), Path::new("src/b.luau")));
        assert!(PathFilter::new(dir.path(), &[]).matches(dir.path(), Path::new("anything")));
    }

    #[test]
    fn display_is_relative_with_forward_slashes() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("src/client")).unwrap();
        let file = dir.path().join("src").join("client").join("a.luau");
        std::fs::write(&file, "").unwrap();
        assert_eq!(display(dir.path(), &file), "src/client/a.luau");
    }
}
