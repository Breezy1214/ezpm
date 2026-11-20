use std::path::{Component, Path, PathBuf};

use crate::error::EzpmError;

/// The files or folders a command was asked to report on. Empty means everything.
pub struct PathFilter {
    roots: Vec<PathBuf>,
}

impl PathFilter {
    /// Fails with a usage error for a path that doesn't exist, so a typo can't make a check
    /// pass by matching nothing.
    pub fn new(project_dir: &Path, paths: &[String]) -> Result<Self, EzpmError> {
        let roots = paths
            .iter()
            .map(|path| {
                let root = absolute(project_dir, Path::new(path));
                if root.exists() {
                    Ok(root)
                } else {
                    Err(EzpmError::usage(format!("path not found: {path}")))
                }
            })
            .collect::<Result<_, _>>()?;
        Ok(Self { roots })
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

/// `path` as an absolute, canonical path. A path that doesn't exist yet is resolved through its
/// nearest existing ancestor, so it shares the form of existing paths (symlinks and Windows
/// 8.3 short names expanded) and prefix comparisons between the two work.
pub fn absolute(project_dir: &Path, path: &Path) -> PathBuf {
    let joined = normalize(&if path.is_absolute() {
        path.to_path_buf()
    } else {
        project_dir.join(path)
    });
    joined
        .ancestors()
        .find_map(|ancestor| {
            let canonical = dunce::canonicalize(ancestor).ok()?;
            let rest = joined.strip_prefix(ancestor).ok()?;
            // Joining an empty rest would add a trailing separator, which a file doesn't have.
            Some(if rest.as_os_str().is_empty() {
                canonical
            } else {
                canonical.join(rest)
            })
        })
        .unwrap_or(joined)
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

        let filter = PathFilter::new(dir.path(), &["./src/client/".to_string()]).expect("filter");
        assert!(filter.matches(dir.path(), Path::new("src/client/a.luau")));
        assert!(!filter.matches(dir.path(), Path::new("src/b.luau")));
        assert!(PathFilter::new(dir.path(), &[])
            .expect("filter")
            .matches(dir.path(), Path::new("anything")));
    }

    #[test]
    fn missing_paths_resolve_through_their_nearest_existing_ancestor() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("real/src")).unwrap();
        let link = dir.path().join("link");
        #[cfg(unix)]
        let linked = std::os::unix::fs::symlink(dir.path().join("real"), &link);
        #[cfg(windows)]
        let linked = std::os::windows::fs::symlink_dir(dir.path().join("real"), &link);
        if linked.is_err() {
            // Windows needs developer mode or admin rights to create a symlink.
            return;
        }

        assert_eq!(
            display(&link, Path::new("src/missing.luau")),
            "src/missing.luau"
        );
        assert_eq!(
            display(&link, &link.join("src/new/missing.luau")),
            "src/new/missing.luau"
        );
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
