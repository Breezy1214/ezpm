use std::io::{self, Write};
use std::path::Path;

pub fn write(path: &Path, contents: impl AsRef<[u8]>) -> io::Result<()> {
    // Replace the symlink's target, not the link, so shared config files stay shared.
    let resolved = std::fs::canonicalize(path).ok();
    let path = resolved.as_deref().unwrap_or(path);
    let parent = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(contents.as_ref())?;
    temp.as_file().sync_all()?;
    // Temp files are created 0600; keep the mode of the file being replaced.
    if let Ok(metadata) = std::fs::metadata(path) {
        temp.as_file().set_permissions(metadata.permissions())?;
    }
    temp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_contents_and_leaves_no_temp_files() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("ezpm.toml");
        std::fs::write(&path, "old").unwrap();

        write(&path, "new").unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn writes_through_symlinks() {
        let dir = tempfile::TempDir::new().unwrap();
        let shared = dir.path().join("shared.luaurc");
        let link = dir.path().join(".luaurc");
        std::fs::write(&shared, "{}").unwrap();
        std::os::unix::fs::symlink(&shared, &link).unwrap();

        write(&link, "{\"aliases\":{}}").unwrap();

        assert!(std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(
            std::fs::read_to_string(&shared).unwrap(),
            "{\"aliases\":{}}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn preserves_existing_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join(".luaurc");
        std::fs::write(&path, "{}").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

        write(&path, "{\"aliases\":{}}").unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o644);
    }
}
