use anyhow::Result;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cli::HookFormat;

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

    let mut lines = Vec::new();
    lines.extend(error_lines(&["check", "--json"], &changed));
    lines.extend(error_lines(
        &["fix-requires", "--check", "--json"],
        &changed,
    ));

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
        .filter(|path| {
            Path::new(path)
                .extension()
                .is_some_and(|ext| ext == "lua" || ext == "luau")
        })
        .map(str::to_string)
        .collect();
    files.sort();
    files.dedup();
    files
}

/// Runs this ezpm with `args` and the files, and renders each error diagnostic as one line.
fn error_lines(args: &[&str], files: &[String]) -> Vec<String> {
    let Ok(exe) = std::env::current_exe() else {
        return Vec::new();
    };
    let Ok(output) = Command::new(exe)
        .args(args)
        .args(files)
        .env("EZPM_NO_UPDATE_CHECK", "1")
        .stdin(std::process::Stdio::null())
        .output()
    else {
        return Vec::new();
    };
    let Ok(document) = serde_json::from_slice::<Value>(&output.stdout) else {
        return Vec::new();
    };
    document["diagnostics"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|diagnostic| diagnostic["severity"] == "error")
        .map(render)
        .collect()
}

fn render(diagnostic: &Value) -> String {
    let location = match (diagnostic["file"].as_str(), diagnostic["line"].as_u64()) {
        (Some(file), Some(line)) => format!("{file}:{line} - "),
        (Some(file), None) => format!("{file} - "),
        _ => String::new(),
    };
    format!(
        "{location}{} ({})",
        diagnostic["message"].as_str().unwrap_or_default(),
        diagnostic["code"].as_str().unwrap_or_default()
    )
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
