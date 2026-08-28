use anyhow::{Context, Result};
use std::path::Path;
use walkdir::WalkDir;

pub fn copy_meta_files(src: &Path, build: &Path) -> Result<usize> {
    let mut count = 0;

    for entry in WalkDir::new(src)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let file_name = entry
            .path()
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        if file_name != "init.meta.json" {
            continue;
        }

        let rel_path = entry.path().strip_prefix(src).with_context(|| {
            format!(
                "Failed to strip prefix '{}' from '{}'",
                src.display(),
                entry.path().display()
            )
        })?;

        let dest = build.join(rel_path);

        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Failed to create parent directories for '{}'",
                    dest.display()
                )
            })?;
        }

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
