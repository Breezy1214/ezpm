use anyhow::{Context, Result};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::output;
use crate::services::{
    rojo_project::{RojoProjectSettings, DEFAULT_PROJECT_TEMPLATE},
    sourcemap, toolchain,
};

fn source_rojo_project() -> PathBuf {
    crate::config::load_config()
        .map(|(config, _)| RojoProjectSettings::from_config(&config).project)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_PROJECT_TEMPLATE))
}

pub fn get_package_dirs(
    aliases: Option<&HashMap<String, String>>,
    src_prefix: &str,
) -> Vec<String> {
    let mut dirs = BTreeSet::new();

    for alias_name in ["Packages", "ServerPackages"] {
        let package_dir = aliases
            .and_then(|aliases| aliases.get(alias_name))
            .and_then(|path| safe_top_level_package_dir(path, src_prefix))
            .unwrap_or_else(|| alias_name.to_string());
        dirs.insert(package_dir);
    }

    dirs.into_iter().collect()
}

fn safe_top_level_package_dir(candidate: &str, src_prefix: &str) -> Option<String> {
    let normalized = candidate.trim().replace('\\', "/");
    let trimmed = normalized.trim_end_matches('/');

    if trimmed.is_empty()
        || trimmed == "."
        || trimmed == ".."
        || trimmed.starts_with('/')
        || trimmed.contains(':')
    {
        return None;
    }

    let mut without_current = trimmed;
    while let Some(rest) = without_current.strip_prefix("./") {
        without_current = rest;
    }

    if without_current.is_empty() || without_current == "." || without_current == ".." {
        return None;
    }

    if without_current.contains('/') || path_is_under_src(without_current, src_prefix) {
        return None;
    }

    Some(without_current.to_string())
}

fn path_is_under_src(path: &str, src_prefix: &str) -> bool {
    let src = src_prefix
        .trim()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string();
    path == src || path.starts_with(&format!("{src}/"))
}

fn safe_package_dir_path(project_root: &Path, src_prefix: &str, pkg_dir: &str) -> Result<PathBuf> {
    let pkg_dir = safe_top_level_package_dir(pkg_dir, src_prefix)
        .with_context(|| format!("Refusing to update unsafe package directory '{pkg_dir}'"))?;
    let target = project_root.join(&pkg_dir);

    let target_metadata = std::fs::symlink_metadata(&target)
        .with_context(|| format!("Failed to inspect package directory '{}'", target.display()))?;
    if target_metadata.file_type().is_symlink() || !target_metadata.is_dir() {
        anyhow::bail!(
            "Refusing to update package path that is not a real directory: '{}'",
            target.display()
        );
    }

    let canonical_root = project_root.canonicalize().with_context(|| {
        format!(
            "Failed to resolve project root '{}'",
            project_root.display()
        )
    })?;
    let canonical_target = target
        .canonicalize()
        .with_context(|| format!("Failed to resolve package directory '{}'", target.display()))?;

    if canonical_target == canonical_root
        || !canonical_target.starts_with(&canonical_root)
        || canonical_target.parent() != Some(canonical_root.as_path())
    {
        anyhow::bail!(
            "Refusing to update unsafe package directory '{}'",
            target.display()
        );
    }

    Ok(target)
}

fn clear_package_files(target: &Path) -> Result<()> {
    for entry in std::fs::read_dir(target)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            clear_package_files(&entry.path())?;
        } else {
            std::fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

fn sync_package_dir(source: &Path, target: &Path) -> Result<()> {
    std::fs::create_dir_all(target)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let dest = target.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_dir() {
            if dest.is_file() {
                std::fs::remove_file(&dest)?;
            }
            sync_package_dir(&entry.path(), &dest)?;
        } else if kind.is_file() {
            if dest.is_dir() {
                std::fs::remove_dir_all(&dest)?;
            }
            let contents = std::fs::read(entry.path())?;
            match std::fs::read(&dest) {
                Ok(existing) if existing == contents => {}
                Ok(_) => std::fs::write(&dest, contents)?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    std::fs::write(&dest, contents)?;
                }
                Err(error) => return Err(error.into()),
            }
        } else {
            anyhow::bail!(
                "Unsupported staged package entry: {}",
                entry.path().display()
            );
        }
    }
    for entry in std::fs::read_dir(target)? {
        let entry = entry?;
        if !source.join(entry.file_name()).exists() {
            if entry.file_type()?.is_dir() {
                clear_package_files(&entry.path())?;
            } else {
                std::fs::remove_file(entry.path())?;
            }
        }
    }
    Ok(())
}

fn validate_package_target(project_root: &Path, src_prefix: &str, pkg_dir: &str) -> Result<()> {
    safe_top_level_package_dir(pkg_dir, src_prefix)
        .with_context(|| format!("Unsafe package directory '{pkg_dir}'"))?;
    let target = project_root.join(pkg_dir);
    match std::fs::symlink_metadata(&target) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
        Ok(_) => {}
    }
    safe_package_dir_path(project_root, src_prefix, pkg_dir)?;
    // Validate the entire target before writing anything, including nested links.
    for entry in walkdir::WalkDir::new(&target).follow_links(false) {
        let entry = entry?;
        anyhow::ensure!(
            entry.file_type().is_dir() || entry.file_type().is_file(),
            "Refusing to update linked or special package entry: {}",
            entry.path().display()
        );
    }
    Ok(())
}

fn ensure_required_tools() -> Result<()> {
    if !Path::new("rokit.toml").exists() {
        return Ok(());
    }

    let contents = std::fs::read_to_string("rokit.toml").context("Failed to read rokit.toml")?;
    let installed_tools =
        toolchain::parse_tool_specs(&contents).context("Failed to parse rokit.toml")?;

    for tool in toolchain::required_runtime_tool_specs() {
        let has_tool = installed_tools
            .iter()
            .any(|existing| existing.name == tool.name);

        if !has_tool {
            output::info(&format!("Adding {} to rokit.toml...", tool.name));
            let result = Command::new("rokit")
                .arg("add")
                .arg(tool.spec.as_str())
                .output()
                .with_context(|| {
                    format!(
                        "Failed to run `rokit add {}`. {}",
                        tool.spec,
                        toolchain::rokit_bootstrap_hint()
                    )
                })?;

            if result.status.success() {
                output::success(&format!("Added {}", tool.name));
            } else {
                let stderr = String::from_utf8_lossy(&result.stderr);
                output::warn(&format!("Failed to add {}: {}", tool.name, stderr.trim()));
            }
        }
    }

    Ok(())
}

#[allow(dead_code)]
fn is_tool_available(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn install_tools(src_prefix: &str, aliases: Option<&HashMap<String, String>>) -> Result<()> {
    ensure_required_tools()?;

    let pb = output::start_spinner("Installing development tools...");

    if output::is_verbose() {
        pb.suspend(|| {});
        let rokit_status = Command::new("rokit")
            .arg("install")
            .status()
            .with_context(|| toolchain::missing_tool_context("rokit"))?;

        if !rokit_status.success() {
            pb.finish_and_clear();
            anyhow::bail!(
                "rokit install failed with exit code: {:?}",
                rokit_status.code()
            );
        }
    } else {
        let rokit_out = Command::new("rokit")
            .arg("install")
            .output()
            .with_context(|| toolchain::missing_tool_context("rokit"))?;

        if !rokit_out.status.success() {
            pb.finish_and_clear();
            anyhow::bail!(
                "rokit install failed with exit code: {:?}",
                rokit_out.status.code()
            );
        }
    }

    pb.finish_and_clear();
    output::success("Rokit tools installed.");

    if Path::new("wally.toml").exists() {
        setup_wally_packages(src_prefix, aliases)?;
    }

    output::success("All tools installed successfully!");
    Ok(())
}

pub fn setup_wally_packages(
    src_prefix: &str,
    aliases: Option<&HashMap<String, String>>,
) -> Result<()> {
    if !Path::new("wally.toml").exists() {
        output::info("No wally.toml found, skipping.");
        return Ok(());
    }

    let package_dirs = get_package_dirs(aliases, src_prefix);
    let source_project = source_rojo_project();

    let pb = output::start_spinner("Setting up Wally packages...");
    let cwd = std::env::current_dir().context("Failed to determine current directory")?;
    for pkg_dir in &package_dirs {
        validate_package_target(&cwd, src_prefix, pkg_dir)?;
    }

    let staging = tempfile::tempdir().context("Failed to create Wally staging directory")?;
    std::fs::copy(cwd.join("wally.toml"), staging.path().join("wally.toml"))?;

    pb.set_message("Installing Wally packages...");

    if output::is_verbose() {
        pb.suspend(|| {});
        let wally_status = Command::new("wally")
            .arg("install")
            .arg("--project-path")
            .arg(staging.path())
            .status()
            .with_context(|| toolchain::missing_tool_context("wally"))?;

        if !wally_status.success() {
            pb.finish_and_clear();
            anyhow::bail!(
                "wally install failed with exit code: {:?}",
                wally_status.code()
            );
        }
    } else {
        let wally_out = Command::new("wally")
            .arg("install")
            .arg("--project-path")
            .arg(staging.path())
            .output()
            .with_context(|| toolchain::missing_tool_context("wally"))?;

        if !wally_out.status.success() {
            pb.finish_and_clear();
            anyhow::bail!(
                "wally install failed with exit code: {:?}: {}",
                wally_out.status.code(),
                String::from_utf8_lossy(&wally_out.stderr).trim()
            );
        }
    }

    pb.set_message("Updating package files...");
    for pkg_dir in &package_dirs {
        validate_package_target(&cwd, src_prefix, pkg_dir)?;
        let source = staging.path().join(pkg_dir);
        std::fs::create_dir_all(&source)?;
        sync_package_dir(&source, &cwd.join(pkg_dir))
            .with_context(|| format!("Failed to update {pkg_dir}"))?;
    }
    std::fs::copy(staging.path().join("wally.lock"), cwd.join("wally.lock"))
        .context("Failed to update wally.lock")?;

    pb.set_message("Generating source map...");

    let sm_result = sourcemap::generate_sourcemap_for_project(&cwd, &source_project)
        .context("Failed to generate sourcemap")?;

    if !sm_result.success {
        pb.finish_and_clear();
        anyhow::bail!("Sourcemap generation failed: {}", sm_result.stderr);
    }
    let type_dirs: Vec<_> = package_dirs
        .iter()
        .filter(|pkg_dir| Path::new(pkg_dir).is_dir())
        .collect();
    let type_generation_error = if type_dirs.is_empty() {
        None
    } else {
        pb.set_message("Setting up package types...");
        let mut command = Command::new("wally-package-types");
        command
            .arg("--sourcemap")
            .arg("sourcemap.json")
            .args(&type_dirs);

        if output::is_verbose() {
            pb.suspend(|| {});
            let status = command
                .status()
                .with_context(|| toolchain::missing_tool_context("wally-package-types"))?;
            (!status.success()).then(String::new)
        } else {
            let result = command
                .output()
                .with_context(|| toolchain::missing_tool_context("wally-package-types"))?;
            if result.status.success() {
                None
            } else {
                let stderr = String::from_utf8_lossy(&result.stderr);
                let stdout = String::from_utf8_lossy(&result.stdout);
                let first_error = stderr
                    .lines()
                    .chain(stdout.lines())
                    .find(|line| line.trim_start().starts_with("error:"))
                    .unwrap_or("wally-package-types exited unsuccessfully")
                    .trim();
                Some(first_error.to_string())
            }
        }
    };

    if let Some(error) = type_generation_error {
        let detail = if error.is_empty() {
            String::new()
        } else {
            format!(" First error: {error}")
        };
        pb.suspend(|| {
            output::warn(&format!(
                "Some package type exports could not be generated; packages remain usable.{detail} Run with --verbose for full diagnostics."
            ))
        });
    }

    pb.set_message("Finalizing...");
    let sm_result2 = sourcemap::generate_sourcemap_for_project(&cwd, &source_project)
        .context("Failed to generate final sourcemap")?;

    pb.finish_and_clear();
    anyhow::ensure!(
        sm_result2.success,
        "Final sourcemap generation failed: {}",
        sm_result2.stderr
    );
    output::success("Wally packages set up!");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::TempDir;

    #[test]
    fn package_sync_preserves_removed_directory_for_rojo_watcher() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("staging");
        let target = dir.path().join("Packages");
        let obsolete = target.join("_Index/example_widget@1.0.0/widget");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&obsolete).unwrap();
        std::fs::write(obsolete.join("init.luau"), "return {}").unwrap();

        sync_package_dir(&source, &target).unwrap();

        assert!(obsolete.is_dir());
        assert_eq!(std::fs::read_dir(obsolete).unwrap().count(), 0);
    }

    #[test]
    fn package_dirs_default_missing_or_unsafe_aliases() {
        let mut aliases = HashMap::new();
        aliases.insert("ProjectRoot".to_string(), ".".to_string());
        aliases.insert("ExplicitRoot".to_string(), "./".to_string());
        aliases.insert("Parent".to_string(), "../ParentProject".to_string());
        aliases.insert("Absolute".to_string(), "/tmp/Packages".to_string());
        aliases.insert("Client".to_string(), "src/client/".to_string());
        aliases.insert("Packages".to_string(), "Packages/".to_string());
        let package_dirs = get_package_dirs(Some(&aliases), "src");

        assert_eq!(
            package_dirs,
            vec!["Packages".to_string(), "ServerPackages".to_string()],
            "only safe top-level package directories should be derived from aliases"
        );
    }

    #[test]
    fn package_dirs_reject_nested_wally_alias_paths() {
        let mut aliases = HashMap::new();
        aliases.insert("Packages".to_string(), "Vendor/Packages/".to_string());
        aliases.insert(
            "ServerPackages".to_string(),
            "src/server-packages/".to_string(),
        );

        let package_dirs = get_package_dirs(Some(&aliases), "src");

        assert_eq!(
            package_dirs,
            vec!["Packages".to_string(), "ServerPackages".to_string()],
            "nested aliases must not collapse to deleting their top-level parent"
        );
    }

    #[test]
    fn package_dirs_accept_windows_style_direct_wally_aliases() {
        let mut aliases = HashMap::new();
        aliases.insert("Packages".to_string(), ".\\Packages\\".to_string());
        aliases.insert("ServerPackages".to_string(), "ServerPackages\\".to_string());

        let package_dirs = get_package_dirs(Some(&aliases), "src");

        assert_eq!(
            package_dirs,
            vec!["Packages".to_string(), "ServerPackages".to_string()],
            "direct Wally package aliases should be accepted with Windows separators"
        );
    }

    #[test]
    fn package_directory_guard_rejects_root_parent_absolute_nested_and_src() {
        let dir = TempDir::new().expect("TempDir::new");
        std::fs::create_dir_all(dir.path().join("src")).expect("create src");
        std::fs::create_dir_all(dir.path().join("Packages").join("Nested"))
            .expect("create nested package path");

        for candidate in [
            ".",
            "./",
            "..",
            "../Packages",
            "..\\Packages",
            "/tmp/Packages",
            "C:\\tmp\\Packages",
            "Packages/Nested",
            "src",
        ] {
            let result = safe_package_dir_path(dir.path(), "src", candidate);
            assert!(
                result.is_err(),
                "{candidate:?} must be rejected as an unsafe package cleanup target"
            );
        }
    }
}
