// Integration tests for `ezpm format` and `ezpm format --check` commands.
//
// Tests verify exit codes and file modification behavior.
// Does NOT assert on specific stdout/stderr text (locked decision).

mod common;

use std::fs;

// ─── Happy path ───────────────────────────────────────────────────────────────

/// `ezpm format` exits 0 when formatting .luau files in src/.
///
/// StyLua formats files in-place; exit 0 even when changes are made
/// (rustfmt/prettier convention — reformatting is success, not failure).
#[test]
fn format_exits_zero() {
    let dir = common::create_project();
    let out = common::run_ezpm(dir.path(), &["format"]);
    common::assert_success(&out);
}

// ─── Exit code contracts (ERR-02) ────────────────────────────────────────────

/// `ezpm format --check` exits non-zero when src/ contains unformatted files.
///
/// This is the CI-friendly mode: checks without modifying files.
/// Unformatted code → non-zero exit (ERR-02 contract).
#[test]
fn format_check_exits_nonzero_when_unformatted() {
    let dir = common::create_project();

    // Write a deliberately unformatted .luau file
    fs::write(
        dir.path().join("src/client/bad.luau"),
        "local    x   =   1\nlocal y=2\n",
    )
    .expect("write unformatted file");

    let out = common::run_ezpm(dir.path(), &["format", "--check"]);
    // StyLua --check exits non-zero when any file is unformatted
    common::assert_failure(&out);
}

/// `ezpm format --check` exits 0 after `ezpm format` has already formatted all files.
///
/// Flow: format in-place first → then check → all clean → exit 0.
#[test]
fn format_check_exits_zero_when_formatted() {
    let dir = common::create_project();

    // First, format the files in-place
    let fmt_out = common::run_ezpm(dir.path(), &["format"]);
    common::assert_success(&fmt_out);

    // Now check — everything should be properly formatted
    let check_out = common::run_ezpm(dir.path(), &["format", "--check"]);
    common::assert_success(&check_out);
}
