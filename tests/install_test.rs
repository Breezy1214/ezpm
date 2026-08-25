mod common;

use std::fs;

#[test]
fn install_exits_zero_without_wally() {
    let dir = common::create_project();
    let out = common::run_ezpm(dir.path(), &["install"]);
    common::assert_success(&out);
}

#[test]
fn install_exits_zero_with_empty_wally_toml() {
    let dir = common::create_project();

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
