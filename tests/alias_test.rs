mod common;

use std::fs;
use tempfile::TempDir;

#[test]
fn alias_list_exits_zero() {
    let dir = common::create_project();
    let out = common::run_ezpm(dir.path(), &["alias", "list"]);
    common::assert_success(&out);
}

#[test]
fn alias_list_with_no_config_exits_zero() {
    let dir = TempDir::new().expect("TempDir::new");
    let out = common::run_ezpm(dir.path(), &["alias", "list"]);
    common::assert_success(&out);
}

#[test]
fn alias_sync_exits_zero() {
    let dir = common::create_project();
    let out = common::run_ezpm(dir.path(), &["alias", "sync"]);
    common::assert_success(&out);

    assert!(
        dir.path().join(".darklua.json").exists(),
        ".darklua.json should exist after alias sync"
    );
}

#[test]
fn alias_sync_with_no_config_exits_zero() {
    let dir = TempDir::new().expect("TempDir::new");
    let out = common::run_ezpm(dir.path(), &["alias", "sync"]);
    common::assert_success(&out);
}

#[test]
fn alias_sync_without_darklua_uses_default() {
    let dir = TempDir::new().expect("TempDir::new");
    fs::write(
        dir.path().join("ezpm.toml"),
        "[project]\nname = \"test-project\"\n\n[aliases]\nClient = \"src/client/\"\n",
    )
    .expect("write ezpm.toml with aliases but no [darklua]");

    let out = common::run_ezpm(dir.path(), &["alias", "sync"]);
    common::assert_success(&out);

    let darklua_json = fs::read_to_string(dir.path().join(".darklua.json"))
        .expect(".darklua.json should be generated from defaults");
    assert!(
        darklua_json.contains("make_assignment_local"),
        "default rules should be used when [darklua] is absent: {darklua_json}"
    );

    let parsed: serde_json::Value =
        serde_json::from_str(&darklua_json).expect(".darklua.json must be valid JSON");
    assert_eq!(
        parsed["loaders"]["**/*.model.json"], "copy",
        "default alias sync output should copy Rojo model files: {darklua_json}"
    );
}

#[test]
fn alias_sync_with_no_aliases_exits_zero() {
    let dir = TempDir::new().expect("TempDir::new");
    fs::write(
        dir.path().join("ezpm.toml"),
        "[project]\nname = \"test-project\"\n",
    )
    .expect("write ezpm.toml without aliases");

    let out = common::run_ezpm(dir.path(), &["alias", "sync"]);
    common::assert_success(&out);
}
