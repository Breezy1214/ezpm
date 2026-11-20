mod common;

use std::fs;

#[test]
fn explain_prints_a_code_and_rejects_unknown_codes_as_usage_errors() {
    let dir = tempfile::TempDir::new().unwrap();

    let out = common::run_ezpm(dir.path(), &["explain", "check.cycle"]);
    common::assert_success(&out);
    assert!(common::stdout(&out).contains("recursive-require"));

    let out = common::run_ezpm(dir.path(), &["explain", "check.cycel"]);
    common::assert_exit(&out, 2);
    assert!(common::stderr(&out).contains("did you mean 'check.cycle'?"));

    let out = common::run_ezpm(dir.path(), &["--json", "explain", "--list"]);
    common::assert_success(&out);
    let codes = common::json_document(&out)["codes"]
        .as_array()
        .unwrap()
        .len();
    assert!(codes >= 10, "{codes} codes");
}

#[test]
fn bad_command_lines_exit_2() {
    let dir = common::create_project();
    common::assert_exit(&common::run_ezpm(dir.path(), &["no-such-command"]), 2);
    common::assert_exit(&common::run_ezpm(dir.path(), &["alias", "add"]), 2);
    common::assert_exit(
        &common::run_ezpm(dir.path(), &["fix-requires", "src/client/init.luau"]),
        2,
    );
}

#[test]
fn json_errors_are_one_document_on_stdout() {
    let dir = common::create_project();
    let out = common::run_ezpm(dir.path(), &["--json", "alias", "add"]);
    common::assert_exit(&out, 2);
    let document = common::json_document(&out);
    assert_eq!(document["error"]["kind"], "usage");
    assert!(document["error"]["hint"].is_string());

    for args in [
        &["--json", "where"][..],
        &["--json", "no-such-command"],
        &["--json"],
    ] {
        let out = common::run_ezpm(dir.path(), args);
        common::assert_exit(&out, 2);
        assert_eq!(
            common::json_document(&out)["error"]["kind"],
            "usage",
            "{args:?}"
        );
    }

    let out = common::run_ezpm(dir.path(), &["build", "-o", "game.rbxl", "--json"]);
    common::assert_exit(&out, 2);
    assert!(common::stderr(&out).contains("ezpm --json build"));
}

#[test]
fn missing_paths_are_usage_errors() {
    let dir = common::create_project();
    common::assert_exit(&common::run_ezpm(dir.path(), &["check", "src/clinet"]), 2);
    common::assert_exit(
        &common::run_ezpm(
            dir.path(),
            &["fix-requires", "--check", "src/clinet/init.luau"],
        ),
        2,
    );
}

#[test]
fn ambiguous_requires_are_reported_on_the_requiring_file() {
    let dir = common::create_project();
    let p = dir.path();
    fs::create_dir_all(p.join("src/shared/A")).unwrap();
    fs::create_dir_all(p.join("src/shared/B")).unwrap();
    fs::write(p.join("src/shared/A/Signal.luau"), "return {}\n").unwrap();
    fs::write(p.join("src/shared/B/Signal.luau"), "return {}\n").unwrap();
    fs::write(
        p.join("src/client/init.luau"),
        "\nreturn require(\"Signal\")\n",
    )
    .unwrap();

    let out = common::run_ezpm(
        p,
        &["--json", "fix-requires", "--check", "src/client/init.luau"],
    );
    common::assert_exit(&out, 1);
    let document = common::json_document(&out);
    let ambiguous = &document["diagnostics"][0];
    assert_eq!(ambiguous["code"], "require.ambiguous");
    assert_eq!(ambiguous["file"], "src/client/init.luau");
    assert_eq!(ambiguous["line"], 2);

    let out = common::run_ezpm(p, &["fix-requires", "--check", "src/shared/A/Signal.luau"]);
    common::assert_success(&out);
}

#[test]
fn config_warnings_reach_json_documents() {
    let dir = common::create_project();
    let mut toml = fs::read_to_string(dir.path().join("ezpm.toml")).unwrap();
    toml.push_str("\n[serve]\nprot = 1\n");
    fs::write(dir.path().join("ezpm.toml"), toml).unwrap();

    let out = common::run_ezpm(dir.path(), &["--json", "where", "src/shared/util.luau"]);
    let document = common::json_document(&out);
    let codes: Vec<&str> = document["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|d| d["code"].as_str())
        .collect();
    assert!(codes.contains(&"config.unknownField"), "{document}");
}

#[test]
fn alias_add_and_remove_take_arguments() {
    let dir = common::create_project();

    let out = common::run_ezpm(
        dir.path(),
        &["--json", "alias", "add", "@Ui", "./src\\ui", "--create-dir"],
    );
    common::assert_success(&out);
    let document = common::json_document(&out);
    assert_eq!(document["alias"]["path"], "src/ui/");
    assert_eq!(document["created_dir"], true);
    assert!(dir.path().join("src/ui").is_dir());
    let luaurc = fs::read_to_string(dir.path().join(".luaurc")).unwrap();
    assert!(luaurc.contains("\"Ui\": \"src/ui/\""), "{luaurc}");

    let out = common::run_ezpm(dir.path(), &["alias", "remove", "Uii"]);
    common::assert_exit(&out, 1);
    assert!(common::stderr(&out).contains("did you mean 'Ui'?"));

    let out = common::run_ezpm(dir.path(), &["alias", "remove", "Ui"]);
    common::assert_success(&out);
    let toml = fs::read_to_string(dir.path().join("ezpm.toml")).unwrap();
    assert!(!toml.contains("Ui ="), "{toml}");
}

#[test]
fn unknown_config_keys_suggest_a_fix_and_annotate_under_github_actions() {
    let dir = common::create_project();
    let mut toml = fs::read_to_string(dir.path().join("ezpm.toml")).unwrap();
    toml.push_str("\n[serve]\nprot = 1\n");
    fs::write(dir.path().join("ezpm.toml"), toml).unwrap();

    let out =
        common::run_ezpm_with_env(dir.path(), &["alias", "list"], [("GITHUB_ACTIONS", "true")]);
    common::assert_success(&out);
    let stderr = common::stderr(&out);
    assert!(stderr.contains("Did you mean 'port'?"), "{stderr}");
    assert!(stderr.contains("(config.unknownField)"), "{stderr}");
    assert!(
        stderr.contains("::warning file=ezpm.toml,title=config.unknownField::"),
        "{stderr}"
    );
}

#[test]
fn invalid_config_is_a_coded_error() {
    let dir = common::create_project();
    fs::write(dir.path().join("ezpm.toml"), "[serve]\nport = \"x\"\n").unwrap();
    let out = common::run_ezpm(dir.path(), &["--json", "alias", "list"]);
    common::assert_exit(&out, 1);
    assert_eq!(
        common::json_document(&out)["diagnostics"][0]["code"],
        "config.invalid"
    );
}

#[test]
fn fix_requires_check_reports_without_writing() {
    let dir = common::create_project();
    let source = dir.path().join("src/client/init.luau");
    let before = fs::read_to_string(&source).unwrap();

    let out = common::run_ezpm(dir.path(), &["fix-requires", "--check", "--color", "never"]);
    common::assert_exit(&out, 1);
    let stderr = common::stderr(&out);
    assert!(
        stderr.contains("src/client/init.luau:1 - error:"),
        "{stderr}"
    );
    assert!(stderr.contains("(require.wouldRewrite)"), "{stderr}");
    assert_eq!(fs::read_to_string(&source).unwrap(), before);

    let out = common::run_ezpm(dir.path(), &["--json", "fix-requires", "--dry-run"]);
    common::assert_success(&out);
    let document = common::json_document(&out);
    assert_eq!(document["changes"][0]["file"], "src/client/init.luau");
    assert_eq!(document["changes"][0]["rewrites"][0]["line"], 1);
    assert_eq!(fs::read_to_string(&source).unwrap(), before);

    let out = common::run_ezpm(
        dir.path(),
        &["fix-requires", "--check", "src/shared/util.luau"],
    );
    common::assert_success(&out);
}

#[test]
fn check_reports_feature_boundaries_with_lines_and_filters_by_path() {
    let dir = common::create_project();
    let p = dir.path();
    fs::create_dir_all(p.join("src/shared/Shop")).unwrap();
    fs::create_dir_all(p.join("src/shared/Inventory")).unwrap();
    fs::write(p.join("src/shared/Inventory/init.luau"), "return {}\n").unwrap();
    fs::write(
        p.join("src/shared/Shop/init.luau"),
        "-- shop\nlocal inv = require(\"@Shared/Inventory\")\nreturn {}\n",
    )
    .unwrap();
    let mut toml = fs::read_to_string(p.join("ezpm.toml")).unwrap();
    toml.push_str("\n[check]\nfeature_roots = [\"src/shared\"]\n");
    fs::write(p.join("ezpm.toml"), toml).unwrap();

    let out = common::run_ezpm(p, &["check", "--json"]);
    common::assert_exit(&out, 1);
    let document = common::json_document(&out);
    let violation = &document["feature_violations"][0];
    assert_eq!(violation["from_feature"], "Shop");
    assert_eq!(violation["to_feature"], "Inventory");
    assert_eq!(violation["line"], 2);
    let codes: Vec<&str> = document["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|d| d["code"].as_str())
        .collect();
    assert!(codes.contains(&"check.featureBoundary"), "{codes:?}");

    let out = common::run_ezpm(p, &["check", "--json", "src/shared/Inventory"]);
    common::assert_success(&out);
    assert!(common::json_document(&out)["feature_violations"]
        .as_array()
        .unwrap()
        .is_empty());

    let out = common::run_ezpm_with_env(p, &["check"], [("GITHUB_ACTIONS", "true")]);
    common::assert_exit(&out, 1);
    assert!(
        common::stderr(&out).contains(
            "::error file=src/shared/Shop/init.luau,line=2,title=check.featureBoundary::"
        ),
        "{}",
        common::stderr(&out)
    );
}

#[test]
fn where_prints_the_instance_and_require() {
    let dir = common::create_project();
    let out = common::run_ezpm(
        dir.path(),
        &[
            "--json",
            "where",
            "src/shared/util.luau",
            "src/missing.luau",
        ],
    );
    common::assert_exit(&out, 1);
    let document = common::json_document(&out);
    let entry = &document["entries"][0];
    assert_eq!(entry["instance_path"], "ReplicatedStorage.Shared.util");
    assert_eq!(entry["class"], "ModuleScript");
    assert_eq!(
        entry["require"],
        "require(\"@game/ReplicatedStorage/Shared/util\")"
    );
    assert_eq!(document["diagnostics"][0]["code"], "where.unmapped");
    assert_eq!(document["diagnostics"][0]["file"], "src/missing.luau");
}

#[test]
fn build_and_sourcemap_pass_arguments_to_rojo() {
    let dir = common::create_project();

    let out = common::run_ezpm(dir.path(), &["sourcemap", "-o", "out.json"]);
    common::assert_success(&out);
    let sourcemap = fs::read_to_string(dir.path().join("out.json")).unwrap();
    assert!(sourcemap.contains("\"className\""), "{sourcemap}");
    assert!(
        dir.path().join(".luaurc").exists(),
        "sourcemap syncs .luaurc first"
    );

    let out = common::run_ezpm(dir.path(), &["--json", "build", "-o", "game.rbxl"]);
    common::assert_success(&out);
    let document = common::json_document(&out);
    assert_eq!(document["command"], "build");
    assert_eq!(document["exit_code"], 0);
    assert!(dir.path().join("game.rbxl").is_file());

    let out = common::run_ezpm(dir.path(), &["build", "--no-such-rojo-flag"]);
    assert!(!out.status.success());
}

#[test]
fn init_agent_rules_writes_agent_files_once() {
    let dir = common::create_project();
    fs::write(dir.path().join("CLAUDE.md"), "# Notes\n").unwrap();

    let out = common::run_ezpm(
        dir.path(),
        &["--json", "init", "--agent-rules", "--agent-hook"],
    );
    common::assert_success(&out);
    assert_eq!(common::json_document(&out)["changed"], true);
    let agents = fs::read_to_string(dir.path().join("AGENTS.md")).unwrap();
    assert!(agents.contains("<!-- ezpm -->"), "{agents}");
    let claude = fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();
    assert_eq!(claude, "# Notes\n@AGENTS.md\n");
    let settings = fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap();
    assert!(settings.contains("ezpm hook stop"), "{settings}");

    let out = common::run_ezpm(
        dir.path(),
        &["--json", "init", "--agent-rules", "--agent-hook"],
    );
    common::assert_success(&out);
    assert_eq!(common::json_document(&out)["changed"], false);
}

#[test]
fn lint_and_format_print_json_documents() {
    let dir = common::create_project();
    let out = common::run_ezpm(dir.path(), &["--json", "format", "--check"]);
    let document = common::json_document(&out);
    assert_eq!(document["check"], true);
    assert_eq!(document["ok"], out.status.success());
}
