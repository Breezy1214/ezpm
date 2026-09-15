mod common;

use std::fs;
use tempfile::TempDir;

#[test]
fn alias_sync_regenerates_luaurc() {
    let dir = TempDir::new().expect("TempDir::new");
    fs::write(
        dir.path().join("ezpm.toml"),
        "[project]\nname = \"test-project\"\n\n[aliases]\nClient = \"src/client/\"\n",
    )
    .expect("write ezpm.toml with aliases");

    let out = common::run_ezpm(dir.path(), &["alias", "sync"]);
    common::assert_success(&out);

    let luaurc =
        fs::read_to_string(dir.path().join(".luaurc")).expect(".luaurc should be generated");
    let parsed: serde_json::Value = serde_json::from_str(&luaurc).expect("valid .luaurc");
    assert_eq!(parsed["aliases"]["Client"], "src/client/");
}
