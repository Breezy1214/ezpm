use anyhow::{Context, Result};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::output;
use crate::services::{sourcemap, toolchain};

// ─── Internal helpers ─────────────────────────────────────────────────────────

/// Derive package directories from the Wally aliases.
pub fn get_package_dirs(
    aliases: Option<&HashMap<String, String>>,
    src_prefix: &str,
) -> Vec<String> {
    let aliases = match aliases {
        Some(a) if !a.is_empty() => a,
        _ => return vec!["Packages".to_string(), "ServerPackages".to_string()],
    };

    let mut dirs = BTreeSet::new();

    for alias_name in ["Packages", "ServerPackages"] {
        let Some(path) = aliases.get(alias_name) else {
            continue;
        };

        if let Some(top_dir) = safe_top_level_package_dir(path, src_prefix) {
            dirs.insert(top_dir);
        }
    }

    if dirs.is_empty() {
        vec!["Packages".to_string(), "ServerPackages".to_string()]
    } else {
        dirs.into_iter().collect()
    }
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

fn removable_package_dir_path(
    project_root: &Path,
    src_prefix: &str,
    pkg_dir: &str,
) -> Result<PathBuf> {
    let pkg_dir = safe_top_level_package_dir(pkg_dir, src_prefix)
        .with_context(|| format!("Refusing to remove unsafe package directory '{pkg_dir}'"))?;
    let target = project_root.join(&pkg_dir);

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
            "Refusing to remove unsafe package directory '{}'",
            target.display()
        );
    }

    Ok(target)
}

/// Check `rokit.toml` for missing required tools and run `rokit add` for each.
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

/// Check whether a tool binary is available in PATH by invoking `--version`
#[allow(dead_code)]
fn is_tool_available(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ─── Public functions ─────────────────────────────────────────────────────────

/// Run `rokit install`, then delegate to `setup_wally_packages`
pub fn install_tools(src_prefix: &str, aliases: Option<&HashMap<String, String>>) -> Result<()> {
    ensure_required_tools()?;

    let pb = output::start_spinner("Installing development tools...");

    // ── 1. Rokit install ────────────────────────────────────────────────────
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

/// Clean and re-install Wally packages from scratch
pub fn setup_wally_packages(
    src_prefix: &str,
    aliases: Option<&HashMap<String, String>>,
) -> Result<()> {
    if !Path::new("wally.toml").exists() {
        output::info("No wally.toml found, skipping.");
        return Ok(());
    }

    let package_dirs = get_package_dirs(aliases, src_prefix);

    let pb = output::start_spinner("Setting up Wally packages...");
    pb.set_message("Clearing current systems...");

    // ── Remove stale artefacts ───────────────────────────────────────────────
    if Path::new("sourcemap.json").exists() {
        std::fs::remove_file("sourcemap.json").context("Failed to remove sourcemap.json")?;
    }

    if Path::new("wally.lock").exists() {
        std::fs::remove_file("wally.lock").context("Failed to remove wally.lock")?;
    }

    let cwd = std::env::current_dir().context("Failed to determine current directory")?;

    for pkg_dir in &package_dirs {
        let pkg_path = cwd.join(pkg_dir);
        if pkg_path.exists() {
            let removable = removable_package_dir_path(&cwd, src_prefix, pkg_dir)?;
            std::fs::remove_dir_all(&removable)
                .with_context(|| format!("Failed to remove {pkg_dir}/"))?;
        }
    }

    // ── Wally install ────────────────────────────────────────────────────────
    pb.set_message("Installing Wally packages...");

    if output::is_verbose() {
        pb.suspend(|| {});
        let wally_status = Command::new("wally")
            .arg("install")
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
            .output()
            .with_context(|| toolchain::missing_tool_context("wally"))?;

        if !wally_out.status.success() {
            pb.finish_and_clear();
            anyhow::bail!(
                "wally install failed with exit code: {:?}",
                wally_out.status.code()
            );
        }
    }

    // ── First sourcemap pass ─────────────────────────────────────────────────
    pb.set_message("Generating source map...");

    let sm_result = sourcemap::generate_sourcemap(&cwd).context("Failed to generate sourcemap")?;

    if !sm_result.success {
        pb.suspend(|| {
            output::warn(&format!(
                "Warning: sourcemap generation failed: {}",
                sm_result.stderr
            ))
        });
    }

    // ── wally-package-types for each package directory ───────────────────────
    for pkg_dir in &package_dirs {
        if Path::new(pkg_dir).exists() {
            pb.set_message(format!("Setting up types for {pkg_dir}..."));

            if output::is_verbose() {
                pb.suspend(|| {});
                let wpt_status = Command::new("wally-package-types")
                    .arg("--sourcemap")
                    .arg("sourcemap.json")
                    .arg(pkg_dir.as_str())
                    .status()
                    .with_context(|| toolchain::missing_tool_context("wally-package-types"))?;

                if !wpt_status.success() {
                    pb.suspend(|| {
                        output::warn(&format!(
                            "wally-package-types failed for {pkg_dir} (types may be incomplete)"
                        ))
                    });
                }
            } else {
                let wpt_out = Command::new("wally-package-types")
                    .arg("--sourcemap")
                    .arg("sourcemap.json")
                    .arg(pkg_dir.as_str())
                    .output()
                    .with_context(|| toolchain::missing_tool_context("wally-package-types"))?;

                if !wpt_out.status.success() {
                    pb.suspend(|| {
                        output::warn(&format!(
                            "wally-package-types failed for {pkg_dir} (types may be incomplete)"
                        ))
                    });
                }
            }
        }
    }

    pb.set_message("Finalizing...");
    let sm_result2 =
        sourcemap::generate_sourcemap(&cwd).context("Failed to generate final sourcemap")?;

    if !sm_result2.success {
        pb.suspend(|| {
            output::warn(&format!(
                "Warning: final sourcemap generation failed: {}",
                sm_result2.stderr
            ))
        });
    }

    pb.finish_and_clear();
    output::success("Wally packages set up!");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::toolchain;
    use std::collections::HashMap;
    use tempfile::TempDir;

    #[test]
    fn required_runtime_toolchain_excludes_optional_lune() {
        let names: Vec<&str> = toolchain::required_runtime_tool_specs()
            .iter()
            .map(|tool| tool.name.as_str())
            .collect();

        assert!(
            !names.contains(&"lune"),
            "install should not require lune in the default runtime toolchain"
        );
        assert!(
            names.contains(&"wally"),
            "install should still require wally"
        );
        assert!(names.contains(&"rojo"), "install should still require rojo");
    }

    #[test]
    fn package_dirs_ignore_aliases_that_resolve_outside_project_children() {
        let mut aliases = HashMap::new();
        aliases.insert("ProjectRoot".to_string(), ".".to_string());
        aliases.insert("ExplicitRoot".to_string(), "./".to_string());
        aliases.insert("Parent".to_string(), "../ParentProject".to_string());
        aliases.insert("Absolute".to_string(), "/tmp/Packages".to_string());
        aliases.insert("Client".to_string(), "src/client/".to_string());
        aliases.insert("Packages".to_string(), "Packages/".to_string());
        aliases.insert(
            "ServerPackages".to_string(),
            "./ServerPackages/".to_string(),
        );

        let package_dirs = get_package_dirs(Some(&aliases), "src");

        assert_eq!(
            package_dirs,
            vec!["Packages".to_string(), "ServerPackages".to_string()],
            "only safe top-level package directories should be derived from aliases"
        );
    }

    #[test]
    fn package_dirs_ignore_non_wally_aliases_outside_src() {
        let mut aliases = HashMap::new();
        aliases.insert("Assets".to_string(), "Assets/".to_string());
        aliases.insert("VendorPackages".to_string(), "Vendor/Packages/".to_string());
        aliases.insert(
            "ReplicatedAssets".to_string(),
            "ReplicatedStorage/Assets/".to_string(),
        );
        aliases.insert("Client".to_string(), "src/client/".to_string());

        let package_dirs = get_package_dirs(Some(&aliases), "src");

        assert_eq!(
            package_dirs,
            vec!["Packages".to_string(), "ServerPackages".to_string()],
            "non-Wally aliases must not become recursive deletion targets"
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
    fn package_dirs_fall_back_to_defaults_when_aliases_only_point_inside_src_or_are_unsafe() {
        let mut aliases = HashMap::new();
        aliases.insert("ProjectRoot".to_string(), ".".to_string());
        aliases.insert("Parent".to_string(), "../Packages".to_string());
        aliases.insert("Client".to_string(), "src/client/".to_string());

        let package_dirs = get_package_dirs(Some(&aliases), "src");

        assert_eq!(
            package_dirs,
            vec!["Packages".to_string(), "ServerPackages".to_string()],
            "unsafe aliases must not replace the default Wally package directories"
        );
    }

    #[test]
    fn package_removal_guard_accepts_only_existing_top_level_children() {
        let dir = TempDir::new().expect("TempDir::new");
        std::fs::create_dir_all(dir.path().join("Packages")).expect("create Packages");

        let safe = removable_package_dir_path(dir.path(), "src", "Packages")
            .expect("Packages should be removable");

        assert_eq!(safe, dir.path().join("Packages"));
    }

    #[test]
    fn package_removal_guard_rejects_root_parent_absolute_nested_and_src() {
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
            let result = removable_package_dir_path(dir.path(), "src", candidate);
            assert!(
                result.is_err(),
                "{candidate:?} must be rejected as an unsafe package removal target"
            );
        }
    }
}
