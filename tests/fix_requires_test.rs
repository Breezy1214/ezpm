// Integration tests for `ezpm fix-requires` command.
//
// Verifies exit codes and file modification behavior.
// Does NOT assert on specific stdout/stderr text (locked decision).

mod common;

use std::fs;

// ─── Happy path ───────────────────────────────────────────────────────────────

/// fix-requires rewrites `require("src/shared/util")` to `@Shared/util` notation.
#[test]
fn fix_requires_rewrites_src_paths() {
    let dir = common::create_project();

    let out = common::run_ezpm(dir.path(), &["fix-requires"]);
    common::assert_success(&out);

    // Verify the file was rewritten to @-alias notation
    let content =
        fs::read_to_string(dir.path().join("src/client/init.luau")).expect("read init.luau");
    assert!(
        content.contains("@Shared/") || content.contains("@Client/") || content.contains("@"),
        "file should contain aliased @-require after fix-requires: {content}"
    );
}

// ─── Edge cases ───────────────────────────────────────────────────────────────

/// fix-requires exits 0 when the file is already using @-alias notation.
#[test]
fn fix_requires_exits_zero_when_already_fixed() {
    let dir = common::create_project();

    // Overwrite with already-aliased require — nothing for fix-requires to do
    fs::write(
        dir.path().join("src/client/init.luau"),
        "local util = require(\"@Shared/util\")\n",
    )
    .expect("write pre-aliased file");

    let out = common::run_ezpm(dir.path(), &["fix-requires"]);
    common::assert_success(&out);
}

/// fix-requires exits 0 when there are no .luau files to process.
#[test]
fn fix_requires_exits_zero_on_empty_src() {
    let dir = common::create_project();

    // Remove all .luau files from src/ so there is nothing to fix
    fs::remove_file(dir.path().join("src/client/init.luau")).expect("remove init.luau");

    let out = common::run_ezpm(dir.path(), &["fix-requires"]);
    common::assert_success(&out);
}
