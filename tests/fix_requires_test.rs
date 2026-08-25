mod common;

use std::fs;

#[test]
fn fix_requires_rewrites_src_paths() {
    let dir = common::create_project();

    let out = common::run_ezpm(dir.path(), &["fix-requires"]);
    common::assert_success(&out);

    let content =
        fs::read_to_string(dir.path().join("src/client/init.luau")).expect("read init.luau");
    assert!(
        content.contains("@Shared/") || content.contains("@Client/") || content.contains("@"),
        "file should contain aliased @-require after fix-requires: {content}"
    );
}

#[test]
fn fix_requires_exits_zero_when_already_fixed() {
    let dir = common::create_project();

    fs::write(
        dir.path().join("src/client/init.luau"),
        "local util = require(\"@Shared/util\")\n",
    )
    .expect("write pre-aliased file");

    let out = common::run_ezpm(dir.path(), &["fix-requires"]);
    common::assert_success(&out);
}

#[test]
fn fix_requires_exits_zero_on_empty_src() {
    let dir = common::create_project();

    fs::remove_file(dir.path().join("src/client/init.luau")).expect("remove init.luau");

    let out = common::run_ezpm(dir.path(), &["fix-requires"]);
    common::assert_success(&out);
}
