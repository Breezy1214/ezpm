mod common;

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn serve_fixes_changed_lua_file_without_a_build_tree() {
    let dir = common::create_project();
    std::fs::write(
        dir.path().join("ezpm.toml"),
        r#"[paths]
src = "src/shared"

[aliases]
Client = "src/client/"
Server = "src/server/"
Shared = "src/shared/"
"#,
    )
    .expect("write config with a narrow source root");
    let source = dir.path().join("src/client/init.luau");

    let log_path = dir.path().join("serve.log");
    let log = std::fs::File::create(&log_path).expect("create serve log");
    let mut child = Command::new(common::ezpm_bin())
        .arg("serve")
        .arg("--verbose")
        .arg("--color")
        .arg("never")
        .arg("--port")
        .arg("44876")
        .current_dir(dir.path())
        .env("EZPM_NO_UPDATE_CHECK", "1")
        .stdout(log.try_clone().expect("clone serve log"))
        .stderr(log)
        .spawn()
        .expect("failed to spawn ezpm serve");

    // The initial require fix lands before the watcher starts, so wait for the banner.
    let ready_deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < ready_deadline
        && !std::fs::read_to_string(&log_path).is_ok_and(|log| log.contains("Press Ctrl-C to stop"))
    {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(
        !dir.path().join("build").exists(),
        "serve created a build tree"
    );
    assert!(
        !dir.path().join("sourcemap.json").exists(),
        "serve persisted its in-memory sourcemap"
    );
    let luaurc = std::fs::read_to_string(dir.path().join(".luaurc"))
        .expect("serve should generate .luaurc from ezpm.toml");
    let luaurc: serde_json::Value = serde_json::from_str(&luaurc).expect("valid .luaurc");
    assert_eq!(luaurc["aliases"]["Shared"], "src/shared/");
    let initial_source = std::fs::read_to_string(&source).expect("read fixed source");
    assert!(initial_source.contains("@game/ReplicatedStorage/Shared/util"));

    let server_script = dir.path().join("src/server/main.server.luau");
    let client_script = dir.path().join("src/client/main.client.lua");
    std::fs::write(&server_script, "return require(\"util\")\n").expect("write server script");
    std::fs::write(&client_script, "return require(\"util\")\n").expect("write client script");
    let suffixed_deadline = Instant::now() + Duration::from_secs(10);
    let mut suffixed_scripts_fixed = false;
    while Instant::now() < suffixed_deadline {
        let server_fixed = std::fs::read_to_string(&server_script)
            .is_ok_and(|contents| contents.contains("@game/ReplicatedStorage/Shared/util"));
        let client_fixed = std::fs::read_to_string(&client_script)
            .is_ok_and(|contents| contents.contains("@game/ReplicatedStorage/Shared/util"));
        if server_fixed && client_fixed {
            suffixed_scripts_fixed = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    std::fs::write(
        &source,
        "local util = require(\"util\")\nlocal watcher_value = 918273\nreturn util\n",
    )
    .expect("update source file");

    let rebuild_deadline = Instant::now() + Duration::from_secs(10);
    let mut rebuilt = false;
    while Instant::now() < rebuild_deadline {
        if std::fs::read_to_string(&source).is_ok_and(|contents| {
            contents.contains("918273") && contents.contains("@game/ReplicatedStorage/Shared/util")
        }) {
            rebuilt = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    let renamed_module = dir.path().join("src/shared/helper.luau");
    std::fs::rename(dir.path().join("src/shared/util.luau"), &renamed_module)
        .expect("rename module");
    let rename_deadline = Instant::now() + Duration::from_secs(10);
    let mut rename_fixed = false;
    while Instant::now() < rename_deadline {
        if std::fs::read_to_string(&source)
            .is_ok_and(|contents| contents.contains("@game/ReplicatedStorage/Shared/helper"))
        {
            rename_fixed = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    #[cfg(unix)]
    let _ = nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(child.id() as i32),
        nix::sys::signal::Signal::SIGTERM,
    );
    #[cfg(windows)]
    let _ = Command::new("taskkill")
        .args(["/PID", &child.id().to_string(), "/T", "/F"])
        .output();
    let _ = child.wait();

    let log = std::fs::read_to_string(&log_path).unwrap_or_default();
    assert!(
        rebuilt,
        "changed Lua file was not fixed\n--- serve output ---\n{log}"
    );
    assert!(
        suffixed_scripts_fixed,
        "requires inside .server/.client scripts were not fixed\n--- serve output ---\n{log}"
    );
    assert!(
        rename_fixed,
        "requires were not repaired after module rename\n--- serve output ---\n{log}"
    );
}

#[test]
fn serve_exits_nonzero_without_config() {
    let dir = tempfile::TempDir::new().expect("TempDir::new failed");

    let output = Command::new(common::ezpm_bin())
        .arg("serve")
        .current_dir(dir.path())
        .env("EZPM_NO_UPDATE_CHECK", "1")
        .output()
        .expect("failed to spawn ezpm serve");

    common::assert_failure(&output);
}

#[cfg(unix)]
#[test]
fn serve_json_streams_only_json_events_and_exits_143_on_sigterm() {
    use std::io::{BufRead, BufReader};

    let dir = common::create_project();
    let mut child = Command::new(common::ezpm_bin())
        .args(["--json", "serve", "--port", "44877"])
        .current_dir(dir.path())
        .env("EZPM_NO_UPDATE_CHECK", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn ezpm serve");

    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });

    let mut events = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        let Ok(line) = rx.recv_timeout(Duration::from_millis(200)) else {
            continue;
        };
        let event: serde_json::Value =
            serde_json::from_str(&line).unwrap_or_else(|e| panic!("not JSON ({e}): {line}"));
        let is_ready = event["event"] == "ready";
        events.push(event);
        if is_ready {
            break;
        }
    }
    assert!(
        events
            .iter()
            .any(|e| e["event"] == "ready" && e["port"] == 44877),
        "no ready event: {events:?}"
    );

    let _ = nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(child.id() as i32),
        nix::sys::signal::Signal::SIGTERM,
    );
    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(143));

    let rest: Vec<serde_json::Value> = rx
        .try_iter()
        .map(|line| {
            serde_json::from_str(&line).unwrap_or_else(|e| panic!("not JSON ({e}): {line}"))
        })
        .collect();
    assert!(
        rest.iter()
            .any(|e| e["event"] == "stopped" && e["reason"] == "terminate"),
        "no stopped event: {rest:?}"
    );
}
