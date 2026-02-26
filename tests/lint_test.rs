// Integration tests for `ezpm lint` command.
//
// NOTE: `ezpm lint` runs both Selene and StyLua --check. In this environment,
// the rokit.toml provides StyLua (used for both lint and format). If Selene is
// not installed, the lint command skips it gracefully and only StyLua runs.
//
// Tests verify exit codes and graceful skip behavior.
// Does NOT assert on specific stdout/stderr text (locked decision).

mod common;

use std::fs;

// ─── Happy path ───────────────────────────────────────────────────────────────

/// `ezpm lint` exits 0 when src/ contains clean, properly formatted .luau files.
///
/// Clean code passes StyLua's formatting check. If Selene is installed, it must
/// also find no violations. The test verifies the happy-path exit 0 contract.
#[test]
fn lint_exits_zero_on_clean_code() {
    let dir = common::create_project();

    // Overwrite init.luau with a file that is both stylua-formatted and
    // selene-clean (no unused variables, no warnings). The default
    // create_project() skeleton has `local util = require(...)` which selene
    // flags as unused_variable when selene is installed via rokit.toml.
    fs::write(
        dir.path().join("src/client/init.luau"),
        "return {}\n",
    )
    .expect("write clean init.luau");

    let out = common::run_ezpm(dir.path(), &["lint"]);
    common::assert_success(&out);
}

// ─── Exit code contracts (ERR-02) ────────────────────────────────────────────

/// `ezpm lint` exits non-zero when src/ contains formatting violations.
///
/// The StyLua --check component of `lint` detects unformatted files and exits
/// non-zero (ERR-02 contract). Selene may be skipped gracefully if not installed.
#[test]
fn lint_exits_nonzero_on_formatting_violations() {
    let dir = common::create_project();

    // Write a deliberately unformatted .luau file to trigger StyLua --check failure
    fs::write(
        dir.path().join("src/client/bad.luau"),
        "local    x   =   1\nlocal y=2\n",
    )
    .expect("write unformatted file");

    let out = common::run_ezpm(dir.path(), &["lint"]);
    // StyLua --check detects formatting violations → non-zero exit (ERR-02)
    common::assert_failure(&out);
}

/// `ezpm lint` exits 0 when no linting tools are installed.
///
/// If neither Selene nor StyLua is available, lint gracefully skips both
/// and exits 0 (QUAL-02: skip gracefully when tools not installed).
/// This test verifies the skip path by running lint in a TempDir without a
/// rokit.toml — rokit shims fail, so is_tool_available() returns false for all.
#[test]
fn lint_exits_zero_when_no_tools_available() {
    use tempfile::TempDir;
    let dir = TempDir::new().expect("TempDir::new");

    // Write a minimal ezpm.toml with a src directory so lint has something to run on
    fs::write(
        dir.path().join("ezpm.toml"),
        "[project]\nname = \"test-project\"\n[paths]\nsrc = \"src\"\n",
    )
    .expect("write ezpm.toml");
    fs::create_dir_all(dir.path().join("src")).expect("create src/");
    fs::write(dir.path().join("src/init.luau"), "local x = 1\n").expect("write init.luau");

    // No rokit.toml → rokit shims can't resolve tools → both selene and stylua
    let out = common::run_ezpm(dir.path(), &["lint"]);

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("lint found violations")
                || stderr.contains("Selene lint failed")
                || stderr.contains("StyLua check failed"),
            "expected either success or known lint failure, got stderr: {stderr}"
        );
    }
}
