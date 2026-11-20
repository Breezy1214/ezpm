use anyhow::Result;
use serde_json::json;
use std::process::Command;

use crate::error::EzpmError;
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
        return Err(EzpmError::tool(
            "No linting tools installed (selene and stylua are both missing)",
        )
        .with_hint(toolchain::tool_install_hint("selene"))
        .into());
    }
    if !has_selene {
        output::warn("Skipping Selene (install Rokit and run `ezpm install`)");
    }
    if !has_stylua {
        output::warn("Skipping StyLua (install Rokit and run `ezpm install`)");
    }

    let mut issues_found = false;
    let mut selene_ok = None;
    let mut stylua_ok = None;

    if has_selene {
        selene::generate_selene_roblox_std();
        let mut cmd = Command::new("selene");
        cmd.arg(src_path);
        let ok = run_tool(cmd, None)?;
        selene_ok = Some(ok);
        if !ok {
            output::error_block(
                "Selene lint failed",
                &format!("Violations found in {}", src_path),
                Some("Fix violations listed above, or run `ezpm format` to auto-fix style issues"),
            );
            issues_found = true;
        }
    }

    if has_stylua {
        stylua_ok = Some(run_tool(stylua(src_path, true), None)?);
    }
    if stylua_ok == Some(false) {
        output::error_block(
            "StyLua formatting check failed",
            &format!("Formatting violations found in {}", src_path),
            Some("Run `ezpm format` to auto-fix"),
        );
        issues_found = true;
    }

    if output::is_json() {
        output::document(&json!({
            "ok": !issues_found,
            "tools": [
                { "tool": "selene", "ran": selene_ok.is_some(), "ok": selene_ok },
                { "tool": "stylua", "ran": stylua_ok.is_some(), "ok": stylua_ok },
            ],
        }));
    }
    if issues_found {
        return Err(EzpmError::reported("lint found violations", 1).into());
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
        return Err(EzpmError::tool("StyLua is not installed")
            .with_hint(toolchain::tool_install_hint("stylua"))
            .into());
    }

    let ok = run_tool(stylua(src_path, check), None)?;
    if output::is_json() {
        output::document(&json!({ "ok": ok, "check": check }));
    }
    if !ok {
        if check {
            output::error_block(
                "Format check failed",
                &format!("Unformatted files found in {}", src_path),
                Some("Run `ezpm format` to auto-fix"),
            );
            return Err(EzpmError::reported("format check failed", 1).into());
        }
        return Err(EzpmError::reported("stylua formatting failed (see output above)", 1).into());
    }

    if check {
        output::success("All files properly formatted!");
    } else {
        output::success("Code formatted successfully!");
    }
    Ok(())
}
