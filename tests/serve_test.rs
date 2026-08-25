mod common;

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn serve_starts_and_shuts_down() {
    let dir = common::create_project();

    let mut child = Command::new(common::ezpm_bin())
        .arg("serve")
        .arg("--port")
        .arg("44872")
        .current_dir(dir.path())
        .env("EZPM_NO_UPDATE_CHECK", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn ezpm serve");

    let stdout = child.stdout.take().expect("child stdout was not piped");
    let mut reader = BufReader::new(stdout);

    let deadline = Instant::now() + Duration::from_secs(30);
    let mut ready = false;

    for line in (&mut reader).lines() {
        if Instant::now() >= deadline {
            break;
        }

        match line {
            Ok(text) => {
                if text.contains("Watching") && text.contains("for changes") {
                    ready = true;
                    break;
                }
            }
            Err(_) => break,
        }
    }

    let _ = child.kill();
    let _ = child.wait();

    drop(dir);

    assert!(
        ready,
        "serve never became ready within 30 seconds — \
         ensure all Rokit tools (rojo, darklua, wally) are installed"
    );
}

#[test]
fn serve_bumps_outdated_rokit_tool() {
    use ezpm::services::toolchain;

    let dir = common::create_project();
    let rokit_path = dir.path().join("rokit.toml");

    let bundled_rojo = toolchain::tool_spec_by_name("rojo")
        .expect("rojo is in the bundled manifest")
        .spec
        .clone();
    let original = std::fs::read_to_string(&rokit_path).expect("read rokit.toml");
    let downgraded = original.replace(&bundled_rojo, "rojo-rbx/rojo@7.0.0");
    assert_ne!(
        downgraded, original,
        "test setup should have downgraded the rojo pin"
    );
    std::fs::write(&rokit_path, &downgraded).expect("write downgraded rokit.toml");

    let mut child = Command::new(common::ezpm_bin())
        .arg("serve")
        .arg("--port")
        .arg("44873")
        .current_dir(dir.path())
        .env("EZPM_NO_UPDATE_CHECK", "1")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn ezpm serve");

    let deadline = Instant::now() + Duration::from_secs(30);
    let mut bumped = false;
    while Instant::now() < deadline {
        if let Ok(contents) = std::fs::read_to_string(&rokit_path) {
            if contents.contains(&bundled_rojo) && !contents.contains("rojo-rbx/rojo@7.0.0") {
                bumped = true;
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    let _ = child.kill();
    let _ = child.wait();
    drop(dir);

    assert!(
        bumped,
        "serve should rewrite the outdated rojo pin to the bundled version ({})",
        bundled_rojo
    );
}

#[test]
fn serve_regenerates_when_rojo_template_changes() {
    let dir = common::create_project();
    let template_path = dir.path().join("default.project.json");
    let generated_path = dir.path().join("build.project.json");

    let mut child = Command::new(common::ezpm_bin())
        .arg("serve")
        .arg("--port")
        .arg("44874")
        .current_dir(dir.path())
        .env("EZPM_NO_UPDATE_CHECK", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn ezpm serve");

    let stdout = child.stdout.take().expect("child stdout was not piped");
    let mut reader = BufReader::new(stdout);
    let ready_deadline = Instant::now() + Duration::from_secs(30);
    let mut ready = false;
    for line in (&mut reader).lines() {
        if Instant::now() >= ready_deadline {
            break;
        }
        match line {
            Ok(text) if text.contains("Watching") && text.contains("for changes") => {
                ready = true;
                break;
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    assert!(ready, "serve did not become ready");

    let updated_template = r#"{
  "name": "test-project",
  "customMetadata": { "preserved": true },
  "tree": {
    "$className": "DataModel",
    "ReplicatedStorage": {
      "Shared": { "$path": "src/shared" }
    }
  }
}
"#;
    std::fs::write(&template_path, updated_template).expect("update Rojo template");

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut regenerated = false;
    while Instant::now() < deadline {
        if let Ok(contents) = std::fs::read_to_string(&generated_path) {
            let parsed: serde_json::Value =
                serde_json::from_str(&contents).expect("generated project should remain JSON");
            if parsed["tree"]["ReplicatedStorage"]["Shared"]["$path"] == "darklua_build/shared"
                && parsed["customMetadata"]["preserved"] == true
            {
                regenerated = true;
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    let _ = child.kill();
    let _ = child.wait();

    assert!(
        regenerated,
        "template change did not regenerate the build project"
    );
    assert_eq!(
        std::fs::read_to_string(template_path).expect("read user template"),
        updated_template,
        "serve must not rewrite the user-owned template"
    );
}

#[test]
fn serve_rebuilds_only_the_changed_lua_file() {
    let dir = common::create_project();
    let source = dir.path().join("src/client/init.luau");
    let built = dir.path().join("darklua_build/client/init.luau");

    let mut child = Command::new(common::ezpm_bin())
        .arg("serve")
        .arg("--port")
        .arg("44875")
        .current_dir(dir.path())
        .env("EZPM_NO_UPDATE_CHECK", "1")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn ezpm serve");

    let ready_deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < ready_deadline && !built.exists() {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(built.exists(), "initial build did not complete");

    std::fs::write(
        &source,
        "local watcher_value = 918273\nreturn watcher_value\n",
    )
    .expect("update source file");

    let rebuild_deadline = Instant::now() + Duration::from_secs(10);
    let mut rebuilt = false;
    while Instant::now() < rebuild_deadline {
        if std::fs::read_to_string(&built).is_ok_and(|contents| contents.contains("918273")) {
            rebuilt = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    let _ = child.kill();
    let _ = child.wait();

    assert!(rebuilt, "changed Lua file was not rebuilt");
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
