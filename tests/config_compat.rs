use ezpm::config::{import_aliases_from_dir, load_config_from_str};
use std::fs;
use tempfile::TempDir;

fn make_temp_dir(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().expect("failed to create temp dir");
    for (name, content) in files {
        fs::write(dir.path().join(name), content).expect("failed to write file");
    }
    dir
}

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
fn missing_toml_falls_back_to_defaults() {
    let (config, warnings) = load_config_from_str("").expect("empty string should succeed");

    assert!(
        warnings.is_empty(),
        "empty input should produce no warnings"
    );
    assert!(
        config.project.is_none(),
        "project should be None by default"
    );
    assert!(config.paths.is_none(), "paths should be None by default");
    assert!(
        config.display.is_none(),
        "display should be None by default"
    );
    assert!(
        config.aliases.is_none(),
        "aliases should be None by default"
    );
    assert!(config.serve.is_none(), "serve should be None by default");
}

#[test]
fn full_config_with_all_sections() {
    let toml = r#"
[project]
name = "my-game"

[paths]
src = "src"
darklua_build = "darklua_build"

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
    assert_eq!(paths.darklua_build, Some("darklua_build".to_string()));

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
fn serve_section_is_optional() {
    let toml = r#"
[project]
name = "test"

[aliases]
Client = "src/client/"
"#;

    let (config, warnings) = load_config_from_str(toml).expect("should parse without serve");
    assert!(
        warnings.is_empty(),
        "no warnings for missing optional serve section"
    );
    assert!(
        config.serve.is_none(),
        "serve should be None when not specified"
    );
}

#[test]
fn rojo_section_and_path_maps_parse_without_warnings() {
    let toml = r#"
[rojo]
project = "config/game.project.json"
generated_project = "build/game.project.json"

[[rojo.path_maps]]
source = "src"
build = "darklua_build"

[[rojo.path_maps]]
source = "common"
build = "darklua_build/common"
"#;

    let (config, warnings) = load_config_from_str(toml).expect("rojo config should parse");
    assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    let rojo = config.rojo.expect("rojo section");
    assert_eq!(rojo.project.as_deref(), Some("config/game.project.json"));
    assert_eq!(
        rojo.generated_project.as_deref(),
        Some("build/game.project.json")
    );
    let maps = rojo.path_maps.expect("path maps");
    assert_eq!(maps.len(), 2);
    assert_eq!(maps[1].source, "common");
    assert_eq!(maps[1].build, "darklua_build/common");
}

#[test]
fn aliases_section_parses_as_hashmap() {
    let toml = r#"
[aliases]
Client = "src/client/"
Server = "src/server/"
Shared = "src/shared/"
Packages = "Packages/"
ServerPackages = "ServerPackages/"
"#;

    let (config, warnings) = load_config_from_str(toml).expect("aliases should parse");
    assert!(warnings.is_empty());

    let aliases = config.aliases.expect("aliases section should be present");
    assert_eq!(aliases.len(), 5, "should have 5 aliases");
    assert_eq!(
        aliases.get("Client"),
        Some(&"src/client/".to_string()),
        "Client alias should match"
    );
}

#[test]
fn darklua_json_alias_auto_import() {
    let darklua_json = r#"{
        "process": [{
            "rule": "convert_require",
            "current": {
                "name": "luau",
                "sources": {
                    "@Client": "src/client/",
                    "@Server": "src/server/"
                }
            },
            "target": {
                "name": "roblox",
                "rojo_sourcemap": "sourcemap.json"
            }
        }]
    }"#;

    let dir = make_temp_dir(&[(".darklua.json", darklua_json)]);

    let aliases = import_aliases_from_dir(dir.path());

    assert_eq!(aliases.len(), 2, "should import 2 aliases");
    assert_eq!(
        aliases.get("Client"),
        Some(&"src/client/".to_string()),
        "Client alias should have @ stripped"
    );
    assert_eq!(
        aliases.get("Server"),
        Some(&"src/server/".to_string()),
        "Server alias should have @ stripped"
    );
}

#[test]
fn luaurc_alias_import_preferred_over_darklua() {
    let luaurc = r#"{
        "aliases": {
            "Client": "src/client/",
            "lune": "~/.lune/.typedefs/0.8"
        }
    }"#;

    let darklua_json = r#"{
        "process": [{
            "rule": "convert_require",
            "current": {
                "name": "luau",
                "sources": {
                    "@Client": "src/client/",
                    "@Server": "src/server/"
                }
            },
            "target": {
                "name": "roblox",
                "rojo_sourcemap": "sourcemap.json"
            }
        }]
    }"#;

    let dir = make_temp_dir(&[(".luaurc", luaurc), (".darklua.json", darklua_json)]);

    let aliases = import_aliases_from_dir(dir.path());

    assert_eq!(aliases.len(), 1, "should have 1 alias (lune skipped)");
    assert_eq!(
        aliases.get("Client"),
        Some(&"src/client/".to_string()),
        "Client alias from .luaurc"
    );
    assert!(
        !aliases.contains_key("lune"),
        "lune typedef path should be skipped"
    );
    assert!(
        !aliases.contains_key("Server"),
        ".darklua.json should not be used when .luaurc exists"
    );
}
