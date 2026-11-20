#![allow(dead_code)]

use ezpm::services::toolchain;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use tempfile::TempDir;

pub fn ezpm_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_ezpm"))
}

pub fn create_project() -> TempDir {
    let dir = TempDir::new().expect("TempDir::new failed");
    let p = dir.path();

    fs::write(
        p.join("ezpm.toml"),
        r#"[project]
name = "test-project"

[paths]
src = "src"

[aliases]
Client = "src/client/"
Server = "src/server/"
Shared = "src/shared/"
Packages = "Packages/"
"#,
    )
    .expect("write ezpm.toml");

    fs::write(
        p.join("default.project.json"),
        r#"{
  "name": "test-project",
  "tree": {
    "$className": "DataModel",
    "ReplicatedStorage": {
      "Shared": { "$path": "src/shared" },
      "Client": { "$path": "src/client" }
    },
    "ServerScriptService": {
      "Server": { "$path": "src/server" }
    }
  }
}
"#,
    )
    .expect("write default.project.json");

    fs::write(
        p.join("rokit.toml"),
        toolchain::render_default_rokit_toml(None),
    )
    .expect("write rokit.toml");

    fs::create_dir_all(p.join("src/client")).expect("create src/client");
    fs::create_dir_all(p.join("src/server")).expect("create src/server");
    fs::create_dir_all(p.join("src/shared")).expect("create src/shared");
    fs::create_dir_all(p.join("Packages")).expect("create Packages");

    fs::write(
        p.join("src/client/init.luau"),
        "local util = require(\"src/shared/util\")\n",
    )
    .expect("write src/client/init.luau");
    fs::write(p.join("src/shared/util.luau"), "return {}\n").expect("write src/shared/util.luau");

    dir
}

pub fn run_ezpm(project_dir: &Path, args: &[&str]) -> Output {
    run_ezpm_with_env(project_dir, args, std::iter::empty::<(&str, &str)>())
}

pub fn run_ezpm_with_env<I, K, V>(project_dir: &Path, args: &[&str], extra_env: I) -> Output
where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<std::ffi::OsStr>,
    V: AsRef<std::ffi::OsStr>,
{
    let mut cmd = Command::new(ezpm_bin());
    cmd.args(args)
        .current_dir(project_dir)
        .stdin(Stdio::null())
        .env("EZPM_NO_UPDATE_CHECK", "1");

    for (key, value) in extra_env {
        cmd.env(key, value);
    }

    cmd.output().expect("failed to spawn ezpm binary")
}

pub fn assert_success(output: &Output) {
    if !output.status.success() {
        eprintln!(
            "--- stdout ---\n{}",
            String::from_utf8_lossy(&output.stdout)
        );
        eprintln!(
            "--- stderr ---\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        panic!("expected exit 0, got {:?}", output.status.code());
    }
}

pub fn assert_exit(output: &Output, code: i32) {
    if output.status.code() != Some(code) {
        eprintln!(
            "--- stdout ---\n{}",
            String::from_utf8_lossy(&output.stdout)
        );
        eprintln!(
            "--- stderr ---\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        panic!("expected exit {code}, got {:?}", output.status.code());
    }
}

/// The single JSON document a `--json` command prints on stdout.
pub fn json_document(output: &Output) -> serde_json::Value {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    assert_eq!(
        lines.len(),
        1,
        "expected one JSON line on stdout, got:\n{stdout}"
    );
    serde_json::from_str(lines[0]).unwrap_or_else(|e| panic!("invalid JSON ({e}): {stdout}"))
}

pub fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

pub fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

pub fn assert_failure(output: &Output) {
    if output.status.success() {
        eprintln!(
            "--- stdout ---\n{}",
            String::from_utf8_lossy(&output.stdout)
        );
        eprintln!(
            "--- stderr ---\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        panic!("expected non-zero exit, got 0");
    }
}
