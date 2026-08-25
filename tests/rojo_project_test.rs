use std::fs;

use ezpm::{
    config::{EzpmConfig, PathsConfig, RojoConfig, RojoPathMapConfig},
    services::rojo_project::{
        generate_build_project, transform_project_template, RojoProjectSettings,
    },
};
use serde_json::{json, Value};
use tempfile::TempDir;

fn map(source: &str, build: &str) -> RojoPathMapConfig {
    RojoPathMapConfig {
        source: source.to_string(),
        build: build.to_string(),
    }
}

#[test]
fn structurally_remaps_exact_root_descendants_and_windows_separators() {
    let input = r#"{
        "name": "game",
        "tree": {
            "$className": "DataModel",
            "Exact": { "$path": "src" },
            "Child": { "$path": "src/shared" },
            "Windows": { "$path": "src\\server\\services" },
            "Trailing": { "$path": "src/client/" },
            "Unrelated": { "$path": "src-old/shared" },
            "Packages": { "$path": "Packages" }
        }
    }"#;

    let (output, count) = transform_project_template(input, &[map("src/", "build\\out/")])
        .expect("project should transform");
    let parsed: Value = serde_json::from_str(&output).expect("output should be JSON");

    assert_eq!(count, 4);
    assert_eq!(parsed["tree"]["Exact"]["$path"], "build/out");
    assert_eq!(parsed["tree"]["Child"]["$path"], "build/out/shared");
    assert_eq!(
        parsed["tree"]["Windows"]["$path"],
        "build/out/server/services"
    );
    assert_eq!(parsed["tree"]["Trailing"]["$path"], "build/out/client");
    assert_eq!(parsed["tree"]["Unrelated"]["$path"], "src-old/shared");
    assert_eq!(parsed["tree"]["Packages"]["$path"], "Packages");
}

#[test]
fn preserves_metadata_unknown_fields_and_nested_json() {
    let input = json!({
        "name": "custom",
        "servePort": 12345,
        "globIgnorePaths": ["**/*.spec.luau"],
        "tree": {
            "$className": "DataModel",
            "$properties": { "StreamingEnabled": true },
            "Workspace": {
                "$properties": { "Gravity": 150 },
                "NestedMetadata": [{"$path": "assets"}, {"custom": true}],
                "Code": { "$path": "src/shared", "$ignoreUnknownInstances": true }
            }
        }
    });

    let (output, count) =
        transform_project_template(&input.to_string(), &[map("src", "darklua_build")])
            .expect("project should transform");
    let parsed: Value = serde_json::from_str(&output).expect("valid output");

    assert_eq!(count, 1);
    assert_eq!(parsed["servePort"], 12345);
    assert_eq!(parsed["globIgnorePaths"], json!(["**/*.spec.luau"]));
    assert_eq!(parsed["tree"]["$properties"]["StreamingEnabled"], true);
    assert_eq!(parsed["tree"]["Workspace"]["$properties"]["Gravity"], 150);
    assert_eq!(
        parsed["tree"]["Workspace"]["NestedMetadata"],
        json!([{"$path": "assets"}, {"custom": true}])
    );
    assert_eq!(
        parsed["tree"]["Workspace"]["Code"]["$ignoreUnknownInstances"],
        true
    );
}

#[test]
fn rejects_invalid_json_and_missing_tree() {
    let invalid = transform_project_template("{", &[map("src", "build")])
        .expect_err("invalid JSON should fail");
    assert!(invalid.to_string().contains("invalid JSON"));

    let missing_tree = transform_project_template(r#"{"name":"game"}"#, &[map("src", "build")])
        .expect_err("missing tree should fail");
    assert!(missing_tree
        .to_string()
        .contains("missing a top-level tree"));

    let invalid_tree = transform_project_template(r#"{"tree":[]}"#, &[map("src", "build")])
        .expect_err("non-object tree should fail");
    assert!(invalid_tree
        .to_string()
        .contains("missing a top-level tree"));
}

#[test]
fn generation_uses_defaults_and_skips_an_unchanged_output() {
    let dir = TempDir::new().expect("temp dir");
    fs::write(
        dir.path().join("default.project.json"),
        r#"{"name":"game","tree":{"Shared":{"$path":"src/shared"}}}"#,
    )
    .expect("write template");
    let settings = RojoProjectSettings::from_config(&EzpmConfig::default());

    let first = generate_build_project(dir.path(), &settings).expect("first generation");
    let second = generate_build_project(dir.path(), &settings).expect("second generation");

    assert!(first.written);
    assert!(!second.written);
    assert_eq!(first.remapped_paths, 1);
    assert_eq!(
        first.generated_project,
        dir.path().join("build.project.json")
    );
    let parsed: Value = serde_json::from_str(
        &fs::read_to_string(&first.generated_project).expect("read generated project"),
    )
    .expect("generated project is JSON");
    assert_eq!(parsed["tree"]["Shared"]["$path"], "darklua_build/shared");
}

#[test]
fn config_supports_custom_template_output_and_path_maps() {
    let dir = TempDir::new().expect("temp dir");
    fs::create_dir(dir.path().join("config")).expect("create config dir");
    fs::write(
        dir.path().join("config/lobby.project.json"),
        r#"{"name":"lobby","tree":{"Game":{"$path":"game"},"Shared":{"$path":"common/shared"}}}"#,
    )
    .expect("write template");
    let config = EzpmConfig {
        paths: Some(PathsConfig {
            src: Some("legacy-src".into()),
            darklua_build: Some("legacy-build".into()),
        }),
        rojo: Some(RojoConfig {
            project: Some("config/lobby.project.json".into()),
            generated_project: Some("generated/lobby.project.json".into()),
            path_maps: Some(vec![
                map("game", "build/game"),
                map("common", "build/common"),
            ]),
        }),
        ..EzpmConfig::default()
    };
    let settings = RojoProjectSettings::from_config(&config);

    let result = generate_build_project(dir.path(), &settings).expect("custom generation");
    let parsed: Value = serde_json::from_str(
        &fs::read_to_string(dir.path().join("generated/lobby.project.json")).expect("read output"),
    )
    .expect("output JSON");

    assert_eq!(result.remapped_paths, 2);
    assert_eq!(parsed["tree"]["Game"]["$path"], "build/game");
    assert_eq!(parsed["tree"]["Shared"]["$path"], "build/common/shared");
}

#[test]
fn generation_reports_a_missing_project() {
    let dir = TempDir::new().expect("temp dir");
    let settings = RojoProjectSettings::from_config(&EzpmConfig::default());
    let error = generate_build_project(dir.path(), &settings).expect_err("missing project fails");
    let message = format!("{error:#}");
    assert!(message.contains("default.project.json"), "{message}");
    assert!(message.contains("missing or unreadable"), "{message}");
}

#[test]
fn generation_never_overwrites_its_template() {
    let dir = TempDir::new().expect("temp dir");
    let project = dir.path().join("game.project.json");
    let original = r#"{"name":"game","tree":{"Code":{"$path":"src"}}}"#;
    fs::write(&project, original).expect("write template");
    let settings = RojoProjectSettings {
        project: "game.project.json".into(),
        generated_project: "game.project.json".into(),
        path_maps: vec![map("src", "build")],
    };

    let error = generate_build_project(dir.path(), &settings).expect_err("same path should fail");
    assert!(error.to_string().contains("must differ"));
    assert_eq!(
        fs::read_to_string(project).expect("read template"),
        original
    );
}

#[test]
fn generation_rejects_parent_paths_without_writing_outside_the_project() {
    let container = TempDir::new().expect("temp dir");
    let project_root = container.path().join("project");
    fs::create_dir(&project_root).expect("create project root");
    fs::write(
        project_root.join("default.project.json"),
        r#"{"name":"game","tree":{}}"#,
    )
    .expect("write template");
    let outside = container.path().join("outside.project.json");
    fs::write(&outside, "do not replace").expect("write sentinel");
    let settings = RojoProjectSettings {
        project: "default.project.json".into(),
        generated_project: "../outside.project.json".into(),
        path_maps: vec![map("src", "build")],
    };

    let error = generate_build_project(&project_root, &settings).expect_err("traversal must fail");
    assert!(error.to_string().contains("project-relative"));
    assert_eq!(
        fs::read_to_string(outside).expect("read sentinel"),
        "do not replace"
    );
}

#[test]
fn generation_rejects_absolute_and_outside_template_paths() {
    let container = TempDir::new().expect("temp dir");
    let project_root = container.path().join("project");
    fs::create_dir(&project_root).expect("create project root");
    let outside_template = container.path().join("outside-template.project.json");
    fs::write(&outside_template, r#"{"name":"outside","tree":{}}"#)
        .expect("write outside template");

    let absolute_output = container.path().join("absolute-output.project.json");
    let absolute_settings = RojoProjectSettings {
        project: "default.project.json".into(),
        generated_project: absolute_output.clone(),
        path_maps: vec![map("src", "build")],
    };
    let error = generate_build_project(&project_root, &absolute_settings)
        .expect_err("absolute output must fail before reading or writing");
    assert!(error.to_string().contains("project-relative"));
    assert!(!absolute_output.exists());

    let windows_absolute_settings = RojoProjectSettings {
        project: "default.project.json".into(),
        generated_project: r"C:\outside.project.json".into(),
        path_maps: vec![map("src", "build")],
    };
    let error = generate_build_project(&project_root, &windows_absolute_settings)
        .expect_err("Windows absolute output must fail on every host");
    assert!(error.to_string().contains("project-relative"));
    assert!(!project_root.join(r"C:\outside.project.json").exists());

    let outside_template_settings = RojoProjectSettings {
        project: "../outside-template.project.json".into(),
        generated_project: "generated.project.json".into(),
        path_maps: vec![map("src", "build")],
    };
    let error = generate_build_project(&project_root, &outside_template_settings)
        .expect_err("outside template must fail");
    assert!(error.to_string().contains("project-relative"));
    assert!(!project_root.join("generated.project.json").exists());
}

#[test]
fn conflicting_duplicate_normalized_path_maps_are_rejected() {
    let input = r#"{"name":"game","tree":{"Code":{"$path":"src/shared"}}}"#;
    let error = transform_project_template(
        input,
        &[map("src/", "build/one"), map("src\\", "build/two")],
    )
    .expect_err("ambiguous duplicate source must fail");

    let message = error.to_string();
    assert!(message.contains("source 'src'"), "{message}");
    assert!(message.contains("build/one"), "{message}");
    assert!(message.contains("build/two"), "{message}");
}

#[test]
fn path_maps_cannot_escape_the_project() {
    let input = r#"{"name":"game","tree":{"Code":{"$path":"src"}}}"#;

    for path_map in [map("../src", "build"), map("src", "../build")] {
        let error = transform_project_template(input, &[path_map])
            .expect_err("path traversal must be rejected");
        assert!(error.to_string().contains("project-relative"));
    }
}

#[test]
fn legacy_paths_config_remains_the_default_mapping() {
    let config = EzpmConfig {
        paths: Some(PathsConfig {
            src: Some("game-src".into()),
            darklua_build: Some("out".into()),
        }),
        ..EzpmConfig::default()
    };
    let settings = RojoProjectSettings::from_config(&config);
    assert_eq!(settings.path_maps, vec![map("game-src", "out")]);
}
