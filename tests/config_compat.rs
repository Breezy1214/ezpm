use ezpm::config::load_config_from_str;

#[test]
fn luau_format_ezpm_toml_loads_without_error() {
    let contents = include_str!("fixtures/ezpm.toml");

    let (config, warnings) = load_config_from_str(contents).expect("should parse without error");

    assert!(
        warnings.is_empty(),
        "valid config should produce no warnings, got: {:?}",
        warnings
    );

    let project = config.project.expect("project section should be present");
    assert_eq!(
        project.name,
        Some("ez-project-manager".to_string()),
        "project name should match"
    );
}

#[test]
fn unknown_fields_produce_warnings_not_errors() {
    let toml = r#"
[project]
name = "test"
unknown_field = "value"
"#;

    let (_, warnings) = load_config_from_str(toml).expect("unknown fields should not cause errors");

    assert!(!warnings.is_empty(), "should have at least one warning");
    assert!(
        warnings.iter().any(|w| w.contains("unknown_field")),
        "warning should mention the unknown field name, got: {:?}",
        warnings
    );
}

#[test]
fn full_config_with_all_sections() {
    let toml = r#"
[project]
name = "my-game"

[paths]
src = "src"

[display]
file_changes = true
docs_enabled = false
logs_enabled = true
check_updates = false

[aliases]
Client = "src/client/"
Server = "src/server/"
Shared = "src/shared/"

[serve]
port = 34872
"#;

    let (config, warnings) = load_config_from_str(toml).expect("full config should parse");
    assert!(warnings.is_empty(), "no warnings for fully-known config");

    let project = config.project.expect("project section");
    assert_eq!(project.name, Some("my-game".to_string()));

    let paths = config.paths.expect("paths section");
    assert_eq!(paths.src, Some("src".to_string()));

    let display = config.display.expect("display section");
    assert_eq!(display.file_changes, Some(true));
    assert_eq!(display.docs_enabled, Some(false));
    assert_eq!(display.logs_enabled, Some(true));
    assert_eq!(display.check_updates, Some(false));

    let aliases = config.aliases.expect("aliases section");
    assert_eq!(aliases.len(), 3, "should have 3 aliases");

    let serve = config.serve.expect("serve section");
    assert_eq!(serve.port, Some(34872));
}

#[test]
fn rojo_project_path_parses_without_warnings() {
    let toml = r#"
[rojo]
project = "config/game.project.json"
"#;

    let (config, warnings) = load_config_from_str(toml).expect("rojo config should parse");
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    let rojo = config.rojo.expect("rojo section");
    assert_eq!(rojo.project.as_deref(), Some("config/game.project.json"));
}
