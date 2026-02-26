use anyhow::{Context, Result};
use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::process::Command;

use crate::output;
use crate::services::sourcemap;

const REQUIRED_TOOLS: &[(&str, &str)] = &[
    ("lune", "lune-org/lune@0.10.4"),
    ("rojo", "rojo-rbx/rojo@7.6.1"),
    ("darklua", "seaofvoices/darklua@0.17.3"),
    ("wally", "UpliftGames/wally@0.3.2"),
    ("wally-package-types", "JohnnyMorganz/wally-package-types@1.6.2"),
];

// ─── Internal helpers ─────────────────────────────────────────────────────────

/// Derive package directories from aliases by filtering out paths under src/
pub fn get_package_dirs(aliases: Option<&HashMap<String, String>>, src_prefix: &str) -> Vec<String> {
    let aliases = match aliases {
        Some(a) if !a.is_empty() => a,
        _ => return vec!["Packages".to_string(), "ServerPackages".to_string()],
    };

    let src_with_slash = format!("{}/", src_prefix.trim_end_matches('/'));
    let mut dirs = BTreeSet::new();

    for path in aliases.values() {
        let trimmed = path.trim_end_matches('/');
        // Skip aliases that are under src/ or equal to src
        if trimmed.starts_with(&src_with_slash) || trimmed == src_prefix {
            continue;
        }
        // Extract top-level directory
        if let Some(top_dir) = trimmed.split('/').next() {
            if !top_dir.is_empty() {
                dirs.insert(top_dir.to_string());
            }
        }
    }

    if dirs.is_empty() {
        vec!["Packages".to_string(), "ServerPackages".to_string()]
    } else {
        dirs.into_iter().collect()
    }
}

/// Check `rokit.toml` for missing required tools and run `rokit add` for each.
fn ensure_required_tools() -> Result<()> {
    if !Path::new("rokit.toml").exists() {
        return Ok(());
    }

    let contents = std::fs::read_to_string("rokit.toml")
        .context("Failed to read rokit.toml")?;

    for &(tool_name, spec) in REQUIRED_TOOLS {
        // Check if the tool name appears as a key (e.g. "lune =")
        let has_tool = contents.lines().any(|line| {
            let trimmed = line.trim();
            trimmed.starts_with(tool_name)
                && trimmed[tool_name.len()..].trim_start().starts_with('=')
        });

        if !has_tool {
            output::info(&format!("Adding {} to rokit.toml...", tool_name));
            let result = Command::new("rokit")
                .arg("add")
                .arg(spec)
                .output()
                .with_context(|| format!("Failed to run rokit add {}", spec))?;

            if result.status.success() {
                output::success(&format!("Added {}", tool_name));
            } else {
                let stderr = String::from_utf8_lossy(&result.stderr);
                output::warn(&format!("Failed to add {}: {}", tool_name, stderr.trim()));
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

/// Check if azul is installed by running `azul --version`.
fn is_azul_installed() -> bool {
    Command::new("azul")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Check if npm is available by running `npm --version`.
fn is_npm_available() -> bool {
    Command::new("npm")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Install azul via npm.
fn install_azul() -> Result<()> {
    let pb = output::start_spinner("Installing Azul via npm...");

    let result = Command::new("npm")
        .args(["install", "-g", "azul"])
        .output()
        .context("Failed to run npm install -g azul")?;

    pb.finish_and_clear();

    if result.status.success() {
        output::success("Azul installed successfully!");
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&result.stderr);
        anyhow::bail!("npm install -g azul failed: {}", stderr.trim());
    }
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
            .context("Failed to run rokit. Is it installed?")?;

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
            .context("Failed to run rokit. Is it installed?")?;

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

    // ── Azul (optional) ──────────────────────────────────────────────────────
    if !is_azul_installed() {
        let install_azul_prompt = inquire::Confirm::new(
            "Would you like to install Azul for two-way Studio sync?",
        )
        .with_default(false)
        .prompt();

        match install_azul_prompt {
            Ok(true) => {
                if is_npm_available() {
                    if let Err(e) = install_azul() {
                        output::warn(&format!("Azul installation failed: {}", e));
                    }
                } else {
                    output::warn("npm is not available. Install Node.js first, then run: npm install -g azul");
                }
            }
            Ok(false) => {
                output::info("Skipping Azul installation.");
            }
            Err(_) => {
                // User cancelled
            }
        }
    }

    output::success("All tools installed successfully!");
    Ok(())
}

/// Clean and re-install Wally packages from scratch
pub fn setup_wally_packages(src_prefix: &str, aliases: Option<&HashMap<String, String>>) -> Result<()> {
    if !Path::new("wally.toml").exists() {
        output::info("No wally.toml found, skipping.");
        return Ok(());
    }

    let package_dirs = get_package_dirs(aliases, src_prefix);

    let pb = output::start_spinner("Setting up Wally packages...");
    pb.set_message("Clearing current systems...");

    // ── Remove stale artefacts ───────────────────────────────────────────────
    if Path::new("sourcemap.json").exists() {
        std::fs::remove_file("sourcemap.json")
            .context("Failed to remove sourcemap.json")?;
    }

    if Path::new("wally.lock").exists() {
        std::fs::remove_file("wally.lock")
            .context("Failed to remove wally.lock")?;
    }

    for pkg_dir in &package_dirs {
        if Path::new(pkg_dir).exists() {
            std::fs::remove_dir_all(pkg_dir)
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
            .context("Failed to run wally. Is it installed? (rokit install)")?;

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
            .context("Failed to run wally. Is it installed? (rokit install)")?;

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

    let cwd =
        std::env::current_dir().context("Failed to determine current directory")?;
    let sm_result = sourcemap::generate_sourcemap(&cwd)
        .context("Failed to generate sourcemap")?;

    if !sm_result.success {
        pb.suspend(|| output::warn(&format!("Warning: sourcemap generation failed: {}", sm_result.stderr)));
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
                    .context("Failed to run wally-package-types")?;

                if !wpt_status.success() {
                    pb.suspend(|| output::warn(&format!(
                        "wally-package-types failed for {pkg_dir} (types may be incomplete)"
                    )));
                }
            } else {
                let wpt_out = Command::new("wally-package-types")
                    .arg("--sourcemap")
                    .arg("sourcemap.json")
                    .arg(pkg_dir.as_str())
                    .output()
                    .context("Failed to run wally-package-types")?;

                if !wpt_out.status.success() {
                    pb.suspend(|| output::warn(&format!(
                        "wally-package-types failed for {pkg_dir} (types may be incomplete)"
                    )));
                }
            }
        }
    }


    pb.set_message("Finalizing...");
    let sm_result2 = sourcemap::generate_sourcemap(&cwd)
        .context("Failed to generate final sourcemap")?;

    if !sm_result2.success {
        pb.suspend(|| output::warn(&format!(
            "Warning: final sourcemap generation failed: {}",
            sm_result2.stderr
        )));
    }

    pb.finish_and_clear();
    output::success("Wally packages set up!");
    Ok(())
}
