mod common;

use std::fs;

#[test]
fn check_circular_dependency_exits_nonzero() {
    let dir = common::create_project();
    let p = dir.path();

    fs::write(
        p.join("src/shared/a.luau"),
        "local b = require(\"@Shared/b\")\nreturn {}\n",
    )
    .expect("write a.luau");

    fs::write(
        p.join("src/shared/b.luau"),
        "local a = require(\"@Shared/a\")\nreturn {}\n",
    )
    .expect("write b.luau");

    fs::write(
        p.join("src/client/init.luau"),
        "local a = require(\"@Shared/a\")\nreturn a\n",
    )
    .expect("write client init");

    let output = common::run_ezpm(p, &["check"]);
    common::assert_failure(&output);
}

#[test]
fn check_architecture_violation_exits_nonzero() {
    let dir = common::create_project();
    let p = dir.path();

    fs::write(
        p.join("src/server/secret.luau"),
        "return { key = \"abc\" }\n",
    )
    .expect("write secret.luau");

    fs::write(
        p.join("src/client/init.luau"),
        "local secret = require(\"@Server/secret\")\nreturn secret\n",
    )
    .expect("write client init");

    fs::write(
        p.join("ezpm.toml"),
        r#"[project]
name = "test-project"

[paths]
src = "src"
darklua_build = "darklua_build"

[aliases]
Client = "src/client/"
Server = "src/server/"
Shared = "src/shared/"
Packages = "Packages/"

[check.layers]
client = "src/client/"
server = "src/server/"
shared = "src/shared/"

[[check.forbid]]
from = "client"
to = "server"
reason = "Client must never import server modules"
"#,
    )
    .expect("write ezpm.toml with forbid rule");

    let output = common::run_ezpm(p, &["check"]);
    common::assert_failure(&output);
}

#[test]
fn check_json_output_is_valid() {
    let dir = common::create_project();
    let p = dir.path();

    fs::write(p.join("src/shared/util.luau"), "return {}\n").expect("write util.luau");
    fs::write(
        p.join("src/client/init.luau"),
        "local util = require(\"@Shared/util\")\nreturn util\n",
    )
    .expect("write init.luau");

    let output = common::run_ezpm(p, &["check", "--json"]);
    common::assert_success(&output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("stdout must be valid JSON");

    assert!(json.get("total_modules").is_some());
    assert!(json.get("total_edges").is_some());
    assert!(json.get("cycles").is_some());
    assert!(json.get("pass").is_some());
    assert_eq!(json["pass"], true);
}
