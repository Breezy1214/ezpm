mod common;

use std::fs;
use tempfile::TempDir;

#[test]
fn init_exits_in_non_tty_environment() {
    let dir = TempDir::new().expect("TempDir::new");
    let out = common::run_ezpm(dir.path(), &["init"]);
    let _ = out.status;
}

#[test]
fn init_dry_run_is_non_interactive_and_does_not_write() {
    let dir = TempDir::new().expect("TempDir::new");
    let template = r#"{
        "name": "existing",
        "tree": {"ReplicatedStorage": {"Shared": {"$path": "src/shared"}}}
    }"#;
    fs::write(dir.path().join("default.project.json"), template).unwrap();

    let out = common::run_ezpm(dir.path(), &["init", "--dry-run", "--color", "never"]);
    common::assert_success(&out);

    assert_eq!(
        fs::read_to_string(dir.path().join("default.project.json")).unwrap(),
        template,
        "dry-run must not overwrite the source Rojo template"
    );
    for path in ["ezpm.toml", ".darklua.json", ".luaurc", "rokit.toml"] {
        assert!(!dir.path().join(path).exists(), "dry-run created {path}");
    }
    let entries = fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    assert_eq!(
        entries,
        [std::ffi::OsString::from("default.project.json")],
        "dry-run must not create auxiliary files"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Would preserve Rojo template"), "{stdout}");
    assert!(stdout.contains("no files were changed"), "{stdout}");
}

#[test]
fn init_dry_run_rejects_ambiguous_source_roots_without_writing() {
    let dir = TempDir::new().expect("TempDir::new");
    fs::write(
        dir.path().join("default.project.json"),
        r#"{"tree":{"A":{"$path":"new/client"},"B":{"$path":"old/server"}}}"#,
    )
    .unwrap();

    let out = common::run_ezpm(dir.path(), &["init", "--dry-run", "--color", "never"]);
    assert!(!out.status.success());
    assert!(!dir.path().join("ezpm.toml").exists());
    assert!(String::from_utf8_lossy(&out.stderr).contains("Source root is ambiguous"));
}
