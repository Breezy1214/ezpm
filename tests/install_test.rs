// Integration tests for `ezpm install` command.
//
// `ezpm install` runs `rokit install`, then conditionally `wally install`
// if `wally.toml` exists. The command gracefully exits 0 in all normal cases
// including when no rokit.toml is present (rokit exits 0 with nothing to do).
//
// Does NOT assert on specific stdout/stderr text (locked decision).

mod common;

use std::fs;
use tempfile::TempDir;

// ─── Happy path ───────────────────────────────────────────────────────────────

/// `ezpm install` exits 0 in a project without wally.toml.
///
/// `rokit install` runs (succeeds or is a no-op) and wally is skipped because
/// `wally.toml` is absent. Validates the happy-path exit 0 contract.
#[test]
fn install_exits_zero_without_wally() {
    let dir = common::create_project();
    // create_project() does not create a wally.toml — wally step is skipped
    let out = common::run_ezpm(dir.path(), &["install"]);
    common::assert_success(&out);
}

/// `ezpm install` exits 0 even when there is no ezpm.toml.
///
/// Config loading falls back to defaults when ezpm.toml is absent; rokit install
/// runs as a no-op if no rokit.toml exists. The command exits 0.
#[test]
fn install_exits_zero_without_config() {
    let dir = TempDir::new().expect("TempDir::new");
    // bare TempDir — no ezpm.toml, no rokit.toml
    let out = common::run_ezpm(dir.path(), &["install"]);
    // rokit exits 0 with nothing to install — overall command succeeds
    common::assert_success(&out);
}

/// `ezpm install` exits 0 with a project that has wally.toml present.
///
/// When wally.toml exists, `wally install` is attempted. Since this is a minimal
/// test project without real package dependencies, wally may fail but is not
/// fatal — the overall `install` command still exits 0.
///
/// NOTE: This test relies on `wally` being available in PATH. If wally is not
/// installed, the test is still valid as a contract test because `wally install`
/// failure handling depends on the error type.
#[test]
fn install_exits_zero_with_empty_wally_toml() {
    let dir = common::create_project();

    // Write a minimal wally.toml with no dependencies — wally install is a no-op
    fs::write(
        dir.path().join("wally.toml"),
        "[package]\nname = \"test/test-project\"\nversion = \"0.1.0\"\nregistry = \"https://github.com/UpliftGames/wally-index\"\n",
    )
    .expect("write wally.toml");

    let out = common::run_ezpm(dir.path(), &["install"]);
    // With no dependencies, wally install should succeed
    common::assert_success(&out);
}
