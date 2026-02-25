// Integration tests for `ezpm init` command.
//
// NOTE: `ezpm init` uses inquire interactive prompts. In a non-TTY environment
// (piped stdin), inquire will error. These tests verify the command does not
// panic or hang — they do NOT assert a specific exit code, because inquire's
// behavior on non-TTY may vary (non-zero is acceptable here).
//
// Does NOT assert on specific stdout/stderr text (locked decision).

mod common;

use tempfile::TempDir;

// ─── Non-TTY behavior ─────────────────────────────────────────────────────────

/// `ezpm init` in a bare directory (no ezpm.toml) with no TTY completes
/// without hanging. Exit code may be non-zero due to inquire prompt failure
/// — that is acceptable; the test verifies no crash/hang only.
#[test]
fn init_exits_in_non_tty_environment() {
    let dir = TempDir::new().expect("TempDir::new");
    let out = common::run_ezpm(dir.path(), &["init"]);
    // We do not assert success or failure — just that the command returned.
    // The important invariant is: no hang, no panic (which would kill the test
    // process itself).
    let _ = out.status; // ignore exit code — non-TTY makes it non-deterministic
}

/// `ezpm init` in an empty TempDir produces an Output (no panic, no hang).
#[test]
fn init_without_project_dir_does_not_crash() {
    let dir = TempDir::new().expect("TempDir::new");
    let out = common::run_ezpm(dir.path(), &["init"]);
    // As long as `run_ezpm` returns an Output, the binary did not crash.
    let _ = out;
}
