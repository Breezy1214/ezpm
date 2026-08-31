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
}
