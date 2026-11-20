use anyhow::Result;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cli::HookFormat;
use crate::commands::{check, fix_requires};
use crate::config;
use crate::diagnostics::Diagnostic;
use crate::services::{require_fixer::is_lua_file, rojo_project::RojoProjectSettings, sourcemap};

/// Agent Stop hook: checks the Luau files changed in git and hands every new error back to the
/// agent as its next prompt. Never fails the agent: problems running the checks are ignored.
pub fn stop(format: HookFormat) -> Result<()> {
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let payload: Value = serde_json::from_str(&input).unwrap_or(Value::Null);

    let project_dir = std::env::var_os("CLAUDE_PROJECT_DIR")
        .map(PathBuf::from)
        .or_else(|| payload["cwd"].as_str().map(PathBuf::from));
    if let Some(dir) = project_dir {
        if std::env::set_current_dir(&dir).is_err() {
            return Ok(());
        }
    }

    let changed = changed_lua_files();
    if changed.is_empty() {
        return Ok(());
    }

    let lines = error_lines(&changed);

    let session = session_id(&payload);
    let seen_file = git(&["rev-parse", "--git-dir"])
        .map(|dir| PathBuf::from(dir.trim()).join(format!("ezpm-hook-{session}")));
    let seen: HashSet<String> = seen_file
        .as_deref()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|text| text.lines().map(str::to_string).collect())
        .unwrap_or_default();
    let new_lines: Vec<String> = lines
        .into_iter()
        .filter(|line| !seen.contains(line))
        .collect();
    if new_lines.is_empty() {
        return Ok(());
    }
    if let Some(path) = &seen_file {
        remember(path, &new_lines);
    }

    let message = format!(
        "ezpm found problems in files you changed. Fix each, or tell the user why it stays:\n{}",
        new_lines.join("\n")
    );
    let response = match format {
        HookFormat::Claude => json!({ "decision": "block", "reason": message }),
        HookFormat::Cursor => json!({ "followup_message": message }),
    };
    println!("{response}");
    Ok(())
}

fn session_id(payload: &Value) -> String {
    ["session_id", "sessionId", "conversation_id"]
        .iter()
        .find_map(|key| payload[key].as_str())
        .unwrap_or("agent")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Luau files modified since HEAD or untracked, relative to the current directory.
fn changed_lua_files() -> Vec<String> {
    let modified = git(&[
        "diff",
        "-z",
        "--name-only",
        "--relative",
        "--diff-filter=d",
        "HEAD",
    ])
    .unwrap_or_default();
    let untracked = git(&["ls-files", "-z", "--others", "--exclude-standard"]).unwrap_or_default();

    let mut files: Vec<String> = modified
        .split('\0')
        .chain(untracked.split('\0'))
        .filter(|path| is_lua_file(Path::new(path)))
        .map(str::to_string)
        .collect();
    files.sort();
    files.dedup();
    files
}

/// Every error `check` and `fix-requires --check` report about `files`, one line each. Both
/// share one Rojo sourcemap.
fn error_lines(files: &[String]) -> Vec<String> {
    let config = match config::load_config() {
        Ok((config, _warnings)) => config,
        Err(error) => {
            let diagnostic =
                Diagnostic::new("config.invalid", error.to_string()).at("ezpm.toml", None);
            return vec![diagnostic.render("error")];
        }
    };
    let Ok(project_dir) = std::env::current_dir() else {
        return Vec::new();
    };
    let rojo = RojoProjectSettings::from_config(&config);
    let index = sourcemap::generate_index(&project_dir, &rojo.project).ok();

    let mut found = Vec::new();
    if let Ok((_, diagnostics)) = check::analyze(&config, &project_dir, index.as_ref(), files) {
        found.extend(diagnostics);
    }
    if let Some(index) = index {
        if let Ok(diagnostics) =
            fix_requires::check_diagnostics(&config, &project_dir, index, files)
        {
            found.extend(diagnostics);
        }
    }
    found
        .iter()
        .filter(|diagnostic| diagnostic.is_error())
        .map(|diagnostic| diagnostic.render("error"))
        .collect()
}

fn remember(path: &Path, lines: &[String]) {
    use std::io::Write as _;
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        for line in lines {
            let _ = writeln!(file, "{line}");
        }
    }
}
