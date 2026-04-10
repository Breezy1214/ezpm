use anyhow::{Context, Result};
use std::path::Path;
use walkdir::WalkDir;

// ─── Public functions ─────────────────────────────────────────────────────────

/// Copy all `init.meta.json` files from `src` to `build`, preserving the
/// relative directory structure.
pub fn copy_meta_files(src: &Path, build: &Path) -> Result<usize> {
    let mut count = 0;

    for entry in WalkDir::new(src)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        // Only copy files named exactly "init.meta.json"
        let file_name = entry
            .path()
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        if file_name != "init.meta.json" {
            continue;
        }

        // Compute relative path from src root
        let rel_path = entry.path().strip_prefix(src).with_context(|| {
            format!(
                "Failed to strip prefix '{}' from '{}'",
                src.display(),
                entry.path().display()
            )
        })?;

        // Compute destination path in build directory
        let dest = build.join(rel_path);

        // Ensure parent directories exist
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Failed to create parent directories for '{}'",
                    dest.display()
                )
            })?;
        }

        // Copy the file
        std::fs::copy(entry.path(), &dest).with_context(|| {
            format!(
                "Failed to copy '{}' to '{}'",
                entry.path().display(),
                dest.display()
            )
        })?;

        count += 1;
    }

    Ok(count)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Helper: create a temp directory with specified files (path relative to
    /// temp root -> file content).
    fn make_temp_tree(files: &[(&str, &str)]) -> TempDir {
        let dir = TempDir::new().expect("failed to create temp dir");
        for (path, content) in files {
            let full: PathBuf = dir.path().join(path);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).expect("failed to create parent dirs");
            }
            std::fs::write(&full, content).expect("failed to write file");
        }
        dir
    }

    #[test]
    fn test_copies_meta_files_to_build() {
        let src = make_temp_tree(&[
            ("a/init.meta.json", r#"{"className":"ModuleScript"}"#),
            ("b/c/init.meta.json", r#"{"className":"Script"}"#),
        ]);
        let build = TempDir::new().expect("failed to create build temp dir");

        let count =
            copy_meta_files(src.path(), build.path()).expect("copy_meta_files must succeed");

        assert_eq!(count, 2, "must copy exactly 2 init.meta.json files");
        assert!(
            build.path().join("a/init.meta.json").exists(),
            "a/init.meta.json must exist in build"
        );
        assert!(
            build.path().join("b/c/init.meta.json").exists(),
            "b/c/init.meta.json must exist in build"
        );
    }

    #[test]
    fn test_no_meta_files_returns_zero() {
        let src = make_temp_tree(&[
            ("a/module.luau", "-- no meta here"),
            ("b/script.lua", "-- also no meta"),
        ]);
        let build = TempDir::new().expect("failed to create build temp dir");

        let count =
            copy_meta_files(src.path(), build.path()).expect("copy_meta_files must succeed");

        assert_eq!(count, 0, "must return 0 when no init.meta.json files exist");
    }

    #[test]
    fn test_preserves_meta_file_content() {
        let content = r#"{"className":"RemoteEvent","properties":{"Name":"MyEvent"}}"#;
        let src = make_temp_tree(&[("shared/init.meta.json", content)]);
        let build = TempDir::new().expect("failed to create build temp dir");

        copy_meta_files(src.path(), build.path()).expect("copy_meta_files must succeed");

        let copied = std::fs::read_to_string(build.path().join("shared/init.meta.json"))
            .expect("read copied file");
        assert_eq!(
            copied, content,
            "copied file content must be identical to source"
        );
    }

    #[test]
    fn test_non_meta_json_files_ignored() {
        let src = make_temp_tree(&[
            ("other.meta.json", r#"{"className":"Folder"}"#),
            ("init.json", r#"{"version":1}"#),
            ("a/init.meta.json", r#"{"className":"ModuleScript"}"#),
        ]);
        let build = TempDir::new().expect("failed to create build temp dir");

        let count =
            copy_meta_files(src.path(), build.path()).expect("copy_meta_files must succeed");

        assert_eq!(
            count, 1,
            "only init.meta.json must be copied, not other.meta.json or init.json"
        );
        assert!(
            !build.path().join("other.meta.json").exists(),
            "other.meta.json must NOT be copied"
        );
        assert!(
            !build.path().join("init.json").exists(),
            "init.json must NOT be copied"
        );
        assert!(
            build.path().join("a/init.meta.json").exists(),
            "a/init.meta.json must be copied"
        );
    }

    #[test]
    fn test_creates_nested_directories() {
        let src = make_temp_tree(&[(
            "deeply/nested/path/init.meta.json",
            r#"{"className":"Script"}"#,
        )]);
        let build = TempDir::new().expect("failed to create build temp dir");

        copy_meta_files(src.path(), build.path()).expect("copy_meta_files must succeed");

        let dest = build.path().join("deeply/nested/path/init.meta.json");
        assert!(
            dest.exists(),
            "deeply nested init.meta.json must be created at {}",
            dest.display()
        );
        assert!(
            dest.parent().expect("parent must exist").is_dir(),
            "parent directories must have been created"
        );
    }
}
