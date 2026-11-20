use anyhow::{Context, Result};
use serde_json::json;
use std::process::{Command, Stdio};

use crate::{
    config::EzpmConfig,
    error::EzpmError,
    output,
    services::{prepare, toolchain},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RojoCommand {
    Build,
    Sourcemap,
}

impl RojoCommand {
    fn name(self) -> &'static str {
        match self {
            RojoCommand::Build => "build",
            RojoCommand::Sourcemap => "sourcemap",
        }
    }
}

/// Syncs `.luaurc`, then runs `rojo <command> <project> <args...>` and exits with Rojo's code.
pub fn run(config: &EzpmConfig, command: RojoCommand, args: &[String]) -> Result<()> {
    let project_dir = std::env::current_dir().context("could not determine current directory")?;
    let rojo = prepare::prepare_project(&project_dir, config)?;
    if !project_dir.join(&rojo.project).is_file() {
        return Err(EzpmError::project(format!(
            "Rojo project '{}' not found",
            rojo.project.display()
        ))
        .with_hint("set [rojo] project in ezpm.toml, or run `ezpm init`")
        .into());
    }

    let mut child = Command::new("rojo");
    child
        .arg(command.name())
        .arg(&rojo.project)
        .args(args)
        .current_dir(&project_dir);
    if output::is_json() {
        // Keep stdout for ezpm's document; a sourcemap printed without -o goes to stderr.
        child.stdout(Stdio::from(std::io::stderr()));
    }

    let status = match child.status() {
        Ok(status) => status,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(EzpmError::tool("Failed to run rojo: it is not installed")
                .with_hint(toolchain::tool_install_hint("rojo"))
                .into());
        }
        Err(error) => return Err(error).context("failed to run rojo"),
    };
    let exit_code = status.code().unwrap_or(1);

    if output::is_json() {
        output::document(&json!({
            "command": command.name(),
            "project": rojo.project,
            "args": args,
            "exit_code": exit_code,
            "ok": status.success(),
        }));
    }

    if !status.success() {
        return Err(EzpmError::reported(
            format!("rojo {} failed (exit code {exit_code})", command.name()),
            exit_code,
        )
        .into());
    }
    Ok(())
}
