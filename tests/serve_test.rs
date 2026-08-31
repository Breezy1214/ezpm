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

    let mut child = Command::new(common::ezpm_bin())
        .arg("serve")
        .arg("--port")
        .arg("44876")
        .current_dir(dir.path())
        .env("EZPM_NO_UPDATE_CHECK", "1")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn ezpm serve");

    let ready_deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < ready_deadline
        && !std::fs::read_to_string(&source)
            .is_ok_and(|contents| contents.contains("@game/ReplicatedStorage/Shared/util"))
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

    let _ = child.kill();
    let _ = child.wait();

    assert!(rebuilt, "changed Lua file was not fixed");
    assert!(
        suffixed_scripts_fixed,
        "requires inside .server/.client scripts were not fixed"
    );
    assert!(
        rename_fixed,
        "requires were not repaired after module rename"
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
