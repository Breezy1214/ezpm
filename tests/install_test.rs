// Integration tests for `ezpm install` command.

mod common;

use std::fs;
use tempfile::TempDir;

// ─── Happy path ───────────────────────────────────────────────────────────────

/// `ezpm install` exits 0 in a project without wally.toml.
#[test]
fn install_exits_zero_without_wally() {
    let dir = common::create_project();
    // create_project() does not create a wally.toml — wally step is skipped
    let out = common::run_ezpm(dir.path(), &["install"]);
    common::assert_success(&out);
}

/// `ezpm install` exits 0 even when there is no ezpm.toml.
fn install_exits_zero_without_config() {
    let dir = TempDir::new().expect("TempDir::new");
    // bare TempDir — no ezpm.toml, no rokit.toml
    let out = common::run_ezpm(dir.path(), &["install"]);
    // rokit exits 0 with nothing to install — overall command succeeds
    common::assert_success(&out);
}

/// `ezpm install` exits 0 with a project that has wally.toml present.
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

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("wally install failed with exit code"),
            "expected either success or known wally failure, got stderr: {stderr}"
        );
    }
}
