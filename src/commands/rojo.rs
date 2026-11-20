use anyhow::{Context, Result};
use serde_json::json;
use std::process::Stdio;

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
    let before_separator = args.iter().take_while(|arg| *arg != "--");
    if before_separator.into_iter().any(|arg| arg == "--json") {
        return Err(EzpmError::usage(format!(
            "--json after `ezpm {}` would be passed to Rojo",
            command.name()
        ))
        .with_hint(format!(
            "put ezpm flags first: ezpm --json {} ...",
            command.name()
        ))
        .into());
    }
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

    let mut child = toolchain::rojo(command.name(), &rojo.project, &project_dir);
    child.args(args);
    if output::is_json() {
        // Keep stdout for ezpm's document; a sourcemap printed without -o goes to stderr.
        child.stdout(Stdio::from(std::io::stderr()));
    }

    let status = child
        .status()
        .map_err(|error| toolchain::spawn_error("rojo", error))?;
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
