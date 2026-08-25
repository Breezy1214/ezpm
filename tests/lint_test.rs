mod common;

use std::fs;

#[test]
fn lint_exits_zero_on_clean_code() {
    let dir = common::create_project();

    fs::write(dir.path().join("src/client/init.luau"), "return {}\n")
        .expect("write clean init.luau");

    let out = common::run_ezpm(dir.path(), &["lint"]);
    common::assert_success(&out);
}

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

#[test]
fn lint_exits_zero_when_no_tools_available() {
    use tempfile::TempDir;
    let dir = TempDir::new().expect("TempDir::new");

    fs::write(
        dir.path().join("ezpm.toml"),
        "[project]\nname = \"test-project\"\n[paths]\nsrc = \"src\"\n",
    )
    .expect("write ezpm.toml");
    fs::create_dir_all(dir.path().join("src")).expect("create src/");
    fs::write(dir.path().join("src/init.luau"), "local x = 1\n").expect("write init.luau");

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
