mod common;

use std::fs;

#[test]
fn format_check_exits_nonzero_when_unformatted() {
    let dir = common::create_project();

    fs::write(
        dir.path().join("src/client/bad.luau"),
        "local    x   =   1\nlocal y=2\n",
    )
    .expect("write unformatted file");

    let out = common::run_ezpm(dir.path(), &["format", "--check"]);
    common::assert_failure(&out);
}
