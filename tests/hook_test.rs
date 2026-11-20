mod common;

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args([
            "-c",
            "user.email=test@example.com",
            "-c",
            "user.name=test",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("git runs");
    assert!(status.success(), "git {args:?} failed");
}

fn run_hook(dir: &Path) -> String {
    let mut child = Command::new(common::ezpm_bin())
        .args(["hook", "stop"])
        .current_dir(dir)
        .env("EZPM_NO_UPDATE_CHECK", "1")
        .env_remove("CLAUDE_PROJECT_DIR")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn hook");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(br#"{"session_id":"test-session"}"#)
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "the hook never fails the agent");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn stop_hook_reports_new_problems_in_changed_files_once() {
    let dir = common::create_project();
    let p = dir.path();
    std::fs::write(p.join("src/client/init.luau"), "return {}\n").unwrap();
    git(p, &["init", "-q"]);
    git(p, &["add", "-A"]);
    git(p, &["commit", "-q", "-m", "init"]);

    assert_eq!(run_hook(p), "", "a clean tree reports nothing");

    std::fs::write(
        p.join("src/shared/a.luau"),
        "return require(\"@game/ReplicatedStorage/Shared/b\")\n",
    )
    .unwrap();
    std::fs::write(
        p.join("src/shared/b.luau"),
        "return require(\"@game/ReplicatedStorage/Shared/a\")\n",
    )
    .unwrap();

    let first = run_hook(p);
    let response: serde_json::Value = serde_json::from_str(first.trim()).expect("one JSON object");
    assert_eq!(response["decision"], "block");
    let reason = response["reason"].as_str().unwrap();
    assert!(reason.contains("(check.cycle)"), "{reason}");
    assert!(reason.contains("src/shared/a.luau:1"), "{reason}");

    assert_eq!(run_hook(p), "", "the same problems are reported only once");

    std::fs::write(
        p.join("src/shared/a.luau"),
        "\n\nreturn require(\"@game/ReplicatedStorage/Shared/b\")\n",
    )
    .unwrap();
    assert_eq!(run_hook(p), "", "a problem that only moved is not new");

    std::fs::write(p.join("ezpm.toml"), "[serve]\nport = \"x\"\n").unwrap();
    let response: serde_json::Value =
        serde_json::from_str(run_hook(p).trim()).expect("a broken config is reported");
    assert!(response["reason"]
        .as_str()
        .unwrap()
        .contains("(config.invalid)"));
}
