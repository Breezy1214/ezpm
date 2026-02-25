// Integration tests for `ezpm alias list` and `ezpm alias sync` commands.
//
// NOTE: `alias add` and `alias remove` use inquire interactive prompts and are
// NOT tested here (Pitfall 3 from RESEARCH.md). Only non-interactive subcommands
// (list and sync) are covered.
//
// Does NOT assert on specific stdout/stderr text (locked decision).

mod common;

use std::fs;
use tempfile::TempDir;

// ─── alias list ───────────────────────────────────────────────────────────────

/// `alias list` exits 0 when aliases are configured.
#[test]
fn alias_list_exits_zero() {
    let dir = common::create_project();
    let out = common::run_ezpm(dir.path(), &["alias", "list"]);
    common::assert_success(&out);
}

/// `alias list` exits 0 when no ezpm.toml exists — config loading returns defaults
/// (no aliases configured is not an error; the command prints "No aliases configured").
#[test]
fn alias_list_with_no_config_exits_zero() {
    let dir = TempDir::new().expect("TempDir::new");
    // bare TempDir — no ezpm.toml
    let out = common::run_ezpm(dir.path(), &["alias", "list"]);
    // load_config() returns Ok(defaults) when file is absent — exit 0
    common::assert_success(&out);
}

// ─── alias sync ───────────────────────────────────────────────────────────────

/// `alias sync` exits 0 and creates `.darklua.json` in the project directory.
#[test]
fn alias_sync_exits_zero() {
    let dir = common::create_project();
    let out = common::run_ezpm(dir.path(), &["alias", "sync"]);
    common::assert_success(&out);

    // Verify .darklua.json was generated
    assert!(
        dir.path().join(".darklua.json").exists(),
        ".darklua.json should exist after alias sync"
    );
}

/// `alias sync` exits 0 when no ezpm.toml exists — load_config() returns defaults,
/// sync sees no aliases and prints "No aliases found in ezpm.toml." then returns Ok.
#[test]
fn alias_sync_with_no_config_exits_zero() {
    let dir = TempDir::new().expect("TempDir::new");
    // bare TempDir — no ezpm.toml
    let out = common::run_ezpm(dir.path(), &["alias", "sync"]);
    // load_config() returns Ok(defaults) when file is absent — exit 0
    common::assert_success(&out);
}

/// `alias sync` exits 0 when ezpm.toml has no aliases — nothing to sync is
/// still a success (no-op path).
#[test]
fn alias_sync_with_no_aliases_exits_zero() {
    let dir = TempDir::new().expect("TempDir::new");
    fs::write(
        dir.path().join("ezpm.toml"),
        "[project]\nname = \"test-project\"\n",
    )
    .expect("write ezpm.toml without aliases");

    let out = common::run_ezpm(dir.path(), &["alias", "sync"]);
    // No aliases is still a success exit (info message printed, returns Ok)
    common::assert_success(&out);
}
