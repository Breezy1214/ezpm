mod common;

#[test]
fn fix_requires_uses_configured_rojo_project() {
    let dir = common::create_project();
    std::fs::write(
        dir.path().join("ezpm.toml"),
        r#"[project]
name = "test-project"

[paths]
src = "src"

[aliases]
Shared = "src/shared/"

[rojo]
project = "custom.project.json"
"#,
    )
    .expect("write ezpm.toml");
    std::fs::write(
        dir.path().join("custom.project.json"),
        r#"{
  "name": "test-project",
  "tree": {
    "$className": "DataModel",
    "ReplicatedFirst": {
      "Libraries": { "$path": "src/shared" }
    },
    "StarterPlayer": {
      "StarterPlayerScripts": {
        "Client": { "$path": "src/client" }
      }
    }
  }
}
"#,
    )
    .expect("write custom project");
    let consumer = dir.path().join("src/client/init.luau");
    std::fs::write(&consumer, "local util = require(\"util\")\n").expect("write consumer");

    let output = common::run_ezpm(dir.path(), &["fix-requires"]);
    common::assert_success(&output);

    let updated = std::fs::read_to_string(consumer).expect("read consumer");
    assert!(updated.contains("@game/ReplicatedFirst/Libraries/util"));
    assert!(!dir.path().join("sourcemap.json").exists());
}

#[test]
fn fix_requires_rewrites_script_and_local_script_consumers() {
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
    let server = dir.path().join("src/server/main.server.luau");
    let client = dir.path().join("src/client/main.client.lua");
    std::fs::write(&server, "return require(\"util\")\n").expect("write server script");
    std::fs::write(&client, "return require(\"util\")\n").expect("write client script");

    let output = common::run_ezpm(dir.path(), &["fix-requires"]);
    common::assert_success(&output);

    for source in [server, client] {
        let updated = std::fs::read_to_string(source).expect("read rewritten script");
        assert!(updated.contains("@game/ReplicatedStorage/Shared/util"));
    }
}
