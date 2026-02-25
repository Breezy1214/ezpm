// Shared integration test helpers for ezpm CLI tests.
//
// Lives in tests/common/mod.rs (subdirectory pattern from Rust Book Ch 11-03)
// so Cargo does NOT compile this as an independent test suite — it appears as
// a module included by each test file via `mod common;`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

// ─── Binary path ──────────────────────────────────────────────────────────────

/// Returns the absolute path to the compiled `ezpm` binary.
///
/// Cargo sets `CARGO_BIN_EXE_ezpm` to the correct path during integration test
/// builds — matches the `[[bin]] name = "ezpm"` entry in `Cargo.toml`.
pub fn ezpm_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_ezpm"))
}

// ─── Project skeleton factory ─────────────────────────────────────────────────

/// Create a minimal but realistic ezpm project skeleton in a fresh TempDir.
///
/// The skeleton includes:
/// - `ezpm.toml` with `[project]`, `[paths]`, and `[aliases]` sections
/// - `default.project.json` (minimal Rojo project)
/// - Directory structure: `src/client/`, `src/server/`, `src/shared/`, `Packages/`
/// - `src/client/init.luau` containing a `require("src/shared/util")` path
///   that `fix-requires` can rewrite to `@Shared/util`
///
/// The caller must keep the returned `TempDir` alive until all assertions
/// complete — dropping it deletes the directory.
pub fn create_project() -> TempDir {
    let dir = TempDir::new().expect("TempDir::new failed");
    let p = dir.path();

    fs::write(
        p.join("ezpm.toml"),
        r#"[project]
name = "test-project"

[paths]
src = "src"
darklua_build = "darklua_build"

[aliases]
Client = "src/client/"
Server = "src/server/"
Shared = "src/shared/"
Packages = "Packages/"
"#,
    )
    .expect("write ezpm.toml");

    fs::write(
        p.join("default.project.json"),
        r#"{
  "name": "test-project",
  "tree": { "$className": "DataModel" }
}
"#,
    )
    .expect("write default.project.json");

    // rokit.toml — required so that rokit shims (stylua, selene, rojo, darklua,
    // wally) resolve to the correct installed binary when the ezpm subprocess
    // runs in this TempDir. Rokit shims walk up from cwd to find rokit.toml.
    // Versions must match what is actually installed in ~/.rokit/tool-storage/.
    fs::write(
        p.join("rokit.toml"),
        "[tools]\n\
         stylua = \"JohnnyMorganz/StyLua@2.3.1\"\n\
         selene = \"Kampfkarren/selene@0.30.0\"\n\
         rojo = \"rojo-rbx/rojo@7.6.1\"\n\
         darklua = \"seaofvoices/darklua@0.17.3\"\n\
         wally = \"UpliftGames/wally@0.3.2\"\n\
         wally-package-types = \"JohnnyMorganz/wally-package-types@1.6.2\"\n\
         lune = \"lune-org/lune@0.10.4\"\n",
    )
    .expect("write rokit.toml");

    fs::create_dir_all(p.join("src/client")).expect("create src/client");
    fs::create_dir_all(p.join("src/server")).expect("create src/server");
    fs::create_dir_all(p.join("src/shared")).expect("create src/shared");
    fs::create_dir_all(p.join("Packages")).expect("create Packages");

    // A require path that fix-requires should rewrite to @Shared/util
    fs::write(
        p.join("src/client/init.luau"),
        "local util = require(\"src/shared/util\")\n",
    )
    .expect("write src/client/init.luau");

    dir
}

// ─── Command runner ───────────────────────────────────────────────────────────

/// Spawn `ezpm <args>` with `cwd` set to `project_dir`.
///
/// - Sets `EZPM_NO_UPDATE_CHECK=1` to suppress background version check noise.
/// - Captures stdout and stderr.
/// - Does NOT panic on non-zero exit — returns the full `Output` for callers to
///   assert on.
pub fn run_ezpm(project_dir: &Path, args: &[&str]) -> Output {
    Command::new(ezpm_bin())
        .args(args)
        .current_dir(project_dir)
        .env("EZPM_NO_UPDATE_CHECK", "1")
        .output()
        .expect("failed to spawn ezpm binary")
}

// ─── Assertion helpers ────────────────────────────────────────────────────────

/// Assert that `output` has a zero exit code.
///
/// On failure, prints captured stdout and stderr before panicking so the test
/// report shows what went wrong.
pub fn assert_success(output: &Output) {
    if !output.status.success() {
        eprintln!(
            "--- stdout ---\n{}",
            String::from_utf8_lossy(&output.stdout)
        );
        eprintln!(
            "--- stderr ---\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        panic!("expected exit 0, got {:?}", output.status.code());
    }
}

/// Assert that `output` has a non-zero exit code.
///
/// On failure (command exited 0 when it should have failed), prints captured
/// stdout and stderr before panicking.
pub fn assert_failure(output: &Output) {
    if output.status.success() {
        eprintln!(
            "--- stdout ---\n{}",
            String::from_utf8_lossy(&output.stdout)
        );
        eprintln!(
            "--- stderr ---\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        panic!("expected non-zero exit, got 0");
    }
}
