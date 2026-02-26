//! Benchmark suite for ezpm Rust implementation.
//!
//! Measures performance of core pure functions:
//! - Config TOML parsing
//! - Require path rewriting (process_file_content)
//! - Config file generation (.darklua.json, .luaurc)
//!
//! Run with: cargo bench --bench rust_bench

use std::collections::HashMap;
use std::time::Instant;

use ezpm::config::load_config_from_str;
use ezpm::services::config_gen::{generate_darklua_json, generate_luaurc};
use ezpm::services::require_fixer::{
    process_file_content,
};

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn bench<F: FnMut()>(name: &str, iterations: u64, mut f: F) {
    // Warm up
    for _ in 0..100 {
        f();
    }

    let start = Instant::now();
    for _ in 0..iterations {
        f();
    }
    let elapsed = start.elapsed();

    let per_iter = elapsed / iterations as u32;
    let ops_per_sec = if per_iter.as_nanos() > 0 {
        1_000_000_000u64 / per_iter.as_nanos() as u64
    } else {
        u64::MAX
    };

    println!(
        "  {:<40} {:>10.3} ms total | {:>10} ns/iter | {:>10} ops/sec  ({} iterations)",
        name,
        elapsed.as_secs_f64() * 1000.0,
        per_iter.as_nanos(),
        format_number(ops_per_sec),
        format_number(iterations),
    );
}

fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    result.chars().rev().collect()
}

// ─── Test data ────────────────────────────────────────────────────────────────

fn sample_toml() -> &'static str {
    r#"[project]
name = "ez-project-manager"

[paths]
src = "src"
darklua_build = "darklua_build"

[display]
file_changes = true
docs_enabled = false
logs_enabled = true

[aliases]
Client = "src/client/"
Server = "src/server/"
Shared = "src/shared/"
Packages = "Packages/"
ServerPackages = "ServerPackages/"

[serve]
port = 34872
"#
}

fn sample_aliases() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("Client".to_string(), "src/client/".to_string());
    m.insert("Server".to_string(), "src/server/".to_string());
    m.insert("Shared".to_string(), "src/shared/".to_string());
    m.insert("Packages".to_string(), "Packages/".to_string());
    m.insert("ServerPackages".to_string(), "ServerPackages/".to_string());
    m
}

fn small_luau_file() -> &'static str {
    r#"local ReplicatedStorage = game:GetService("ReplicatedStorage")

local PlayerModule = require("src/client/PlayerModule")
local Utils = require("src/shared/Utils")
local Config = require("src/shared/Config")
local ServerApi = require("src/server/ServerApi")
local PackageA = require("Packages/PackageA")
local Init = require("src/client/init")

return {}
"#
}

fn large_luau_file() -> String {
    let mut content = String::new();
    content.push_str("-- Auto-generated large file for benchmarking\n");
    content.push_str("local ReplicatedStorage = game:GetService(\"ReplicatedStorage\")\n\n");

    for i in 0..200 {
        let alias_paths = [
            "src/client/",
            "src/server/",
            "src/shared/",
        ];
        let path = alias_paths[i % alias_paths.len()];
        content.push_str(&format!(
            "local Module{i} = require(\"{path}Module{i}\")\n"
        ));
    }

    content.push_str("\nreturn {}\n");
    content
}

// ─── Build helpers matching require_fixer internals ───────────────────────────

fn build_sorted_src_aliases(aliases: &HashMap<String, String>, src_prefix: &str) -> Vec<(String, String)> {
    let prefix = format!("{src_prefix}/");
    let mut list: Vec<(String, String)> = aliases
        .iter()
        .filter(|(_, path)| path.starts_with(&prefix) || *path == src_prefix)
        .map(|(name, path)| {
            let real_path = if path.ends_with('/') {
                path.clone()
            } else {
                format!("{path}/")
            };
            (format!("@{name}/"), real_path)
        })
        .collect();
    list.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));
    list
}

fn build_skip_list(aliases: &HashMap<String, String>, src_prefix: &str) -> Vec<String> {
    let prefix = format!("{src_prefix}/");
    let mut skip = vec![
        "@self/".to_string(),
        "@self".to_string(),
        "@game/".to_string(),
        "@game".to_string(),
    ];
    for (name, path) in aliases {
        if !path.starts_with(&prefix) && path != src_prefix {
            skip.push(format!("@{name}/"));
        }
    }
    skip
}

fn build_inverted_aliases(aliases: &HashMap<String, String>) -> Vec<(String, String)> {
    let mut inverted: Vec<(String, String)> = aliases
        .iter()
        .map(|(name, path)| {
            let real_path = if path.ends_with('/') {
                path.clone()
            } else {
                format!("{path}/")
            };
            (real_path, format!("@{name}/"))
        })
        .collect();
    inverted.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    inverted
}

// ─── Main ─────────────────────────────────────────────────────────────────────

fn main() {
    println!();
    println!("╔══════════════════════════════════════════════════════════════════════╗");
    println!("║                    ezpm Rust Benchmark Suite                        ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝");
    println!();

    let iterations = 10_000u64;
    let large_iterations = 100u64;

    // ── 1. Config TOML parsing ────────────────────────────────────────────────
    println!("── Config TOML Parsing ──────────────────────────────────────────────");
    let toml_input = sample_toml();

    bench("parse ezpm.toml (small)", iterations, || {
        let _ = load_config_from_str(toml_input);
    });

    // Larger TOML with many aliases
    let large_toml = {
        let mut s = sample_toml().to_string();
        for i in 0..50 {
            s.push_str(&format!("Alias{i} = \"src/module{i}/\"\n"));
        }
        s
    };
    bench("parse ezpm.toml (50 extra aliases)", iterations, || {
        let _ = load_config_from_str(&large_toml);
    });

    println!();

    // ── 2. Require path rewriting ─────────────────────────────────────────────
    println!("── Require Path Rewriting ─────────────────────────────────────────");
    let aliases = sample_aliases();
    let src_prefix = "src";
    let sorted_aliases = build_sorted_src_aliases(&aliases, src_prefix);
    let skip_list = build_skip_list(&aliases, src_prefix);
    let inverted_aliases = build_inverted_aliases(&aliases);

    let small_content = small_luau_file();
    bench("process_file_content (6 requires)", iterations, || {
        let _ = process_file_content(
            small_content,
            &sorted_aliases,
            &skip_list,
            src_prefix,
            None,
            &inverted_aliases,
        );
    });

    let large_content = large_luau_file();
    bench("process_file_content (200 requires)", large_iterations, || {
        let _ = process_file_content(
            &large_content,
            &sorted_aliases,
            &skip_list,
            src_prefix,
            None,
            &inverted_aliases,
        );
    });

    println!();

    // ── 3. Config generation ──────────────────────────────────────────────────
    println!("── Config File Generation ─────────────────────────────────────────");

    bench("generate .darklua.json", iterations, || {
        let _ = generate_darklua_json();
    });

    bench("generate .luaurc (5 aliases)", iterations, || {
        let _ = generate_luaurc(&aliases);
    });

    // Larger alias set
    let mut large_aliases = aliases.clone();
    for i in 0..50 {
        large_aliases.insert(format!("Alias{i}"), format!("src/module{i}/"));
    }
    bench("generate .luaurc (55 aliases)", iterations, || {
        let _ = generate_luaurc(&large_aliases);
    });

    println!();

    // ── Summary ───────────────────────────────────────────────────────────────
    println!("── Overall Timing ─────────────────────────────────────────────────");

    // End-to-end: parse config + rewrite 200 requires + generate configs
    bench("full pipeline (parse + 200 rewrites + gen configs)", large_iterations, || {
        let _ = load_config_from_str(sample_toml());
        let _ = process_file_content(
            &large_content,
            &sorted_aliases,
            &skip_list,
            src_prefix,
            None,
            &inverted_aliases,
        );
        let _ = generate_darklua_json();
        let _ = generate_luaurc(&aliases);
    });

    println!();
}
