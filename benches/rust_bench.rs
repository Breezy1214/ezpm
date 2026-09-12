use std::collections::HashMap;
use std::time::Instant;

use ezpm::config::load_config_from_str;
use ezpm::services::config_gen::generate_luaurc;
use ezpm::services::require_fixer::{process_file_content, FixContext};

fn bench<F: FnMut()>(name: &str, iterations: u64, mut f: F) {
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
        "  {:<50} {:>10.3} ms total | {:>10} ns/iter | {:>10} ops/sec  ({} iterations)",
        name,
        elapsed.as_secs_f64() * 1000.0,
        per_iter.as_nanos(),
        format_number(ops_per_sec),
        format_number(iterations),
    );
}

fn bench_compare<F1: FnMut(), F2: FnMut()>(name: &str, iterations: u64, mut old: F1, mut new: F2) {
    for _ in 0..100 {
        old();
        new();
    }

    let start_old = Instant::now();
    for _ in 0..iterations {
        old();
    }
    let elapsed_old = start_old.elapsed();

    let start_new = Instant::now();
    for _ in 0..iterations {
        new();
    }
    let elapsed_new = start_new.elapsed();

    let old_ns = elapsed_old.as_nanos() as f64 / iterations as f64;
    let new_ns = elapsed_new.as_nanos() as f64 / iterations as f64;
    let speedup = if new_ns > 0.0 {
        old_ns / new_ns
    } else {
        f64::INFINITY
    };

    println!(
        "  {:<50} old: {:>8.0} ns/iter | new: {:>8.0} ns/iter | speedup: {:.2}x",
        name, old_ns, new_ns, speedup,
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

fn sample_toml() -> &'static str {
    r#"[project]
name = "ez-project-manager"

[paths]
src = "src"

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
        let alias_paths = ["src/client/", "src/server/", "src/shared/"];
        let path = alias_paths[i % alias_paths.len()];
        content.push_str(&format!("local Module{i} = require(\"{path}Module{i}\")\n"));
    }

    content.push_str("\nreturn {}\n");
    content
}

fn main() {
    println!();
    println!("╔══════════════════════════════════════════════════════════════════════╗");
    println!("║                    ezpm Rust Benchmark Suite                        ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝");
    println!();

    let iterations = 10_000u64;
    let large_iterations = 100u64;

    println!("── Config TOML Parsing ──────────────────────────────────────────────");
    let toml_input = sample_toml();

    bench("parse ezpm.toml (small)", iterations, || {
        let _ = load_config_from_str(toml_input);
    });

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

    println!("── Require Path Rewriting ─────────────────────────────────────────");
    let aliases = sample_aliases();
    let context_dir = tempfile::TempDir::new().expect("context temp dir");
    let parse_sourcemap = || {
        ezpm::services::sourcemap::SourcemapIndex::parse(
            context_dir.path(),
            br#"{"name":"Game","className":"DataModel"}"#,
        )
        .expect("parse sourcemap")
    };
    let fix_ctx = FixContext::new(context_dir.path(), &aliases, parse_sourcemap());

    let small_content = small_luau_file();
    bench("process_file_content (6 requires)", iterations, || {
        let _ = process_file_content(small_content, &fix_ctx, None);
    });

    let large_content = large_luau_file();
    bench(
        "process_file_content (200 requires)",
        large_iterations,
        || {
            let _ = process_file_content(&large_content, &fix_ctx, None);
        },
    );

    println!();

    println!("── Config File Generation ─────────────────────────────────────────");

    bench("generate .luaurc (5 aliases)", iterations, || {
        let _ = generate_luaurc(&aliases);
    });

    let mut large_aliases = aliases.clone();
    for i in 0..50 {
        large_aliases.insert(format!("Alias{i}"), format!("src/module{i}/"));
    }
    bench("generate .luaurc (55 aliases)", iterations, || {
        let _ = generate_luaurc(&large_aliases);
    });

    println!();

    println!("── Optimization: FixContext caching (#2) ────────────────────────────");

    bench_compare(
        "fix_single_file rebuild vs cached (6 requires)",
        iterations,
        || {
            let rebuilt = FixContext::new(context_dir.path(), &aliases, parse_sourcemap());
            let _ = process_file_content(small_content, &rebuilt, None);
        },
        || {
            let _ = process_file_content(small_content, &fix_ctx, None);
        },
    );

    println!();

    println!("── Overall Timing ─────────────────────────────────────────────────");

    bench(
        "full pipeline (parse + 200 rewrites + gen configs)",
        large_iterations,
        || {
            let _ = load_config_from_str(sample_toml());
            let _ = process_file_content(&large_content, &fix_ctx, None);
            let _ = generate_luaurc(&aliases);
        },
    );

    println!();
}
