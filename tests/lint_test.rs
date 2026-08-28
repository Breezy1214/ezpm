mod common;

use std::fs;

#[test]
fn lint_exits_nonzero_on_formatting_violations() {
    let dir = common::create_project();

    fs::write(
        dir.path().join("src/client/bad.luau"),
        "local    x   =   1\nlocal y=2\n",
    )
    .expect("write unformatted file");

    let out = common::run_ezpm(dir.path(), &["lint"]);
    common::assert_failure(&out);
}
