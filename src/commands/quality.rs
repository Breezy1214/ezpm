use anyhow::{Context, Result};
use std::process::Command;

use crate::output;
use crate::services::toolchain;

fn is_tool_available(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn lint(src_path: &str) -> Result<()> {
    let has_selene = is_tool_available("selene");
    let has_stylua = is_tool_available("stylua");

    if !has_selene {
        output::info("Skipping Selene (install Rokit and run `ezpm install`)");
    }
    if !has_stylua {
        output::info("Skipping StyLua (install Rokit and run `ezpm install`)");
    }

    if !has_selene && !has_stylua {
        output::info("No linting tools installed.");
        return Ok(());
    }

    let mut issues_found = false;

    if has_selene {
        if output::is_verbose() {
            let selene_status = Command::new("selene")
                .arg(src_path)
                .status()
                .with_context(|| toolchain::missing_tool_context("selene"))?;

            if !selene_status.success() {
                output::error_block(
                    "Selene lint failed",
                    &format!("Violations found in {}", src_path),
                    Some("Fix violations listed above, or run `ezpm format` to auto-fix style issues"),
                );
                issues_found = true;
            }
        } else {
            let selene_out = Command::new("selene")
                .arg(src_path)
                .output()
                .with_context(|| toolchain::missing_tool_context("selene"))?;

            if !selene_out.status.success() {
                let stderr = String::from_utf8_lossy(&selene_out.stderr);
                if !stderr.trim().is_empty() {
                    eprint!("{}", stderr);
                }
                let stdout = String::from_utf8_lossy(&selene_out.stdout);
                if !stdout.trim().is_empty() {
                    print!("{}", stdout);
                }
                output::error_block(
                    "Selene lint failed",
                    &format!("Violations found in {}", src_path),
                    Some("Fix violations listed above, or run `ezpm format` to auto-fix style issues"),
                );
                issues_found = true;
            }
        }
    }

    if has_stylua {
        if output::is_verbose() {
            let stylua_status = Command::new("stylua")
                .arg("--check")
                .arg(src_path)
                .status()
                .with_context(|| toolchain::missing_tool_context("stylua"))?;

            if !stylua_status.success() {
                output::error_block(
                    "StyLua formatting check failed",
                    &format!("Formatting violations found in {}", src_path),
                    Some("Run `ezpm format` to auto-fix"),
                );
                issues_found = true;
            }
        } else {
            let stylua_out = Command::new("stylua")
                .arg("--check")
                .arg(src_path)
                .output()
                .with_context(|| toolchain::missing_tool_context("stylua"))?;

            if !stylua_out.status.success() {
                let stderr = String::from_utf8_lossy(&stylua_out.stderr);
                if !stderr.trim().is_empty() {
                    eprint!("{}", stderr);
                }
                let stdout = String::from_utf8_lossy(&stylua_out.stdout);
                if !stdout.trim().is_empty() {
                    print!("{}", stdout);
                }
                output::error_block(
                    "StyLua formatting check failed",
                    &format!("Formatting violations found in {}", src_path),
                    Some("Run `ezpm format` to auto-fix"),
                );
                issues_found = true;
            }
        }
    }

    if issues_found {
        anyhow::bail!("lint found violations");
    }
    output::success("All code quality checks passed!");
    Ok(())
}

pub fn format_code(src_path: &str, check: bool) -> Result<()> {
    if !is_tool_available("stylua") {
        output::info("StyLua is not installed.");
        output::hint(&toolchain::tool_install_hint("stylua"));
        return Ok(());
    }

    let mut cmd = Command::new("stylua");
    if check {
        cmd.arg("--check");
    }
    cmd.arg(src_path);

    if output::is_verbose() {
        let stylua_status = cmd
            .status()
            .with_context(|| toolchain::missing_tool_context("stylua"))?;
        if !stylua_status.success() {
            if check {
                output::error_block(
                    "Format check failed",
                    &format!("Unformatted files found in {}", src_path),
                    Some("Run `ezpm format` to auto-fix"),
                );
                anyhow::bail!("format check failed");
            } else {
                anyhow::bail!(
                    "stylua formatting failed with exit code: {:?}",
                    stylua_status.code()
                );
            }
        }
    } else {
        let stylua_out = cmd
            .output()
            .with_context(|| toolchain::missing_tool_context("stylua"))?;

        if !stylua_out.status.success() {
            if check {
                let stderr = String::from_utf8_lossy(&stylua_out.stderr);
                if !stderr.trim().is_empty() {
                    eprint!("{}", stderr);
                }
                let stdout = String::from_utf8_lossy(&stylua_out.stdout);
                if !stdout.trim().is_empty() {
                    print!("{}", stdout);
                }
                output::error_block(
                    "Format check failed",
                    &format!("Unformatted files found in {}", src_path),
                    Some("Run `ezpm format` to auto-fix"),
                );
                anyhow::bail!("format check failed");
            } else {
                anyhow::bail!(
                    "stylua formatting failed with exit code: {:?}",
                    stylua_out.status.code()
                );
            }
        }
    }

    if check {
        output::success("All files properly formatted!");
    } else {
        output::success("Code formatted successfully!");
    }
    Ok(())
}

pub fn docs(docs_enabled: bool) -> Result<()> {
    if !docs_enabled {
        output::info("Documentation is not set up for this project.");
        output::hint("Set docs_enabled = true in ezpm.toml [display] section.");
        return Ok(());
    }

    Command::new("moonwave")
        .arg("dev")
        .status()
        .context("Failed to run moonwave. Install it with Rokit, then try again.")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stylua_install_hint_comes_from_shared_manifest() {
        let stylua = toolchain::tool_spec_by_name("stylua").expect("stylua exists in toolchain");

        assert_eq!(
            toolchain::tool_install_hint("stylua"),
            format!(
                "Install Rokit, then run `rokit add {}` or `ezpm install`.",
                stylua.spec
            ),
            "quality hints should always use the shared rokit manifest version"
        );
    }
}
