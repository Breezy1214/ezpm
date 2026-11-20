use anyhow::Result;
use std::process::Command;

use crate::output;
use crate::services::selene;
use crate::services::toolchain::{self, is_tool_available, run_tool};

fn stylua(src_path: &str, check: bool) -> Command {
    let mut cmd = Command::new("stylua");
    if check {
        cmd.arg("--check");
    }
    cmd.arg(src_path);
    cmd
}

pub fn lint(src_path: &str) -> Result<()> {
    let has_selene = is_tool_available("selene");
    let has_stylua = is_tool_available("stylua");

    if !has_selene && !has_stylua {
        output::hint(&toolchain::tool_install_hint("selene"));
        anyhow::bail!("No linting tools installed (selene and stylua are both missing)");
    }
    if !has_selene {
        output::warn("Skipping Selene (install Rokit and run `ezpm install`)");
    }
    if !has_stylua {
        output::warn("Skipping StyLua (install Rokit and run `ezpm install`)");
    }

    let mut issues_found = false;

    if has_selene {
        selene::generate_selene_roblox_std();
        let mut cmd = Command::new("selene");
        cmd.arg(src_path);
        if !run_tool(cmd, None)? {
            output::error_block(
                "Selene lint failed",
                &format!("Violations found in {}", src_path),
                Some("Fix violations listed above, or run `ezpm format` to auto-fix style issues"),
            );
            issues_found = true;
        }
    }

    if has_stylua && !run_tool(stylua(src_path, true), None)? {
        output::error_block(
            "StyLua formatting check failed",
            &format!("Formatting violations found in {}", src_path),
            Some("Run `ezpm format` to auto-fix"),
        );
        issues_found = true;
    }

    if issues_found {
        anyhow::bail!("lint found violations");
    }
    if has_selene && has_stylua {
        output::success("All code quality checks passed!");
    } else {
        output::success("Available code quality checks passed.");
    }
    Ok(())
}

pub fn format_code(src_path: &str, check: bool) -> Result<()> {
    if !is_tool_available("stylua") {
        output::hint(&toolchain::tool_install_hint("stylua"));
        anyhow::bail!("StyLua is not installed");
    }

    if !run_tool(stylua(src_path, check), None)? {
        if check {
            output::error_block(
                "Format check failed",
                &format!("Unformatted files found in {}", src_path),
                Some("Run `ezpm format` to auto-fix"),
            );
            anyhow::bail!("format check failed");
        }
        anyhow::bail!("stylua formatting failed (see output above)");
    }

    if check {
        output::success("All files properly formatted!");
    } else {
        output::success("Code formatted successfully!");
    }
    Ok(())
}
