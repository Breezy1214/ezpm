use anyhow::{Context, Result};
use owo_colors::{OwoColorize, Stream};

use crate::output;

// ─── Menu items ───────────────────────────────────────────────────────────────

const MENU_ITEMS: &[(&str, &str)] = &[
    ("init           Create a new EZPM project", "init"),
    ("serve          Start file watcher + DarkLua + Rojo", "serve"),
    ("fix-requires   Rewrite require paths to @alias notation", "fix-requires"),
    ("install        Install tools and packages", "install"),
    (
        "setup-wally-packages   Clean + install + type Wally packages",
        "setup-wally-packages",
    ),
    ("alias          Manage path aliases (add/remove/list/sync)", "alias-menu"),
    ("lint           Run Selene and StyLua checks", "lint"),
    ("format         Format source with StyLua", "format"),
    ("docs           Launch Moonwave docs server", "docs"),
    ("exit", "exit"),
];

// ─── ASCII logo ───────────────────────────────────────────────────────────────
fn print_logo(version: &str) {
    println!();
    println!("{}", "  ███████╗███████╗██████╗ ███╗   ███╗".if_supports_color(Stream::Stdout, |t| t.cyan()));
    println!("{}", "  ██╔════╝╚══███╔╝██╔══██╗████╗ ████║".if_supports_color(Stream::Stdout, |t| t.cyan()));
    println!("{}", "  █████╗    ███╔╝ ██████╔╝██╔████╔██║".if_supports_color(Stream::Stdout, |t| t.cyan()));
    println!("{}", "  ██╔══╝   ███╔╝  ██╔═══╝ ██║╚██╔╝██║".if_supports_color(Stream::Stdout, |t| t.cyan()));
    println!("{}", "  ███████╗███████╗██║     ██║ ╚═╝ ██║".if_supports_color(Stream::Stdout, |t| t.cyan()));
    println!("{}", "  ╚══════╝╚══════╝╚═╝     ╚═╝     ╚═╝".if_supports_color(Stream::Stdout, |t| t.cyan()));
    println!("{}", format!("                              v{}", version).if_supports_color(Stream::Stdout, |t| t.cyan()));
    println!();
}

// ─── Interactive menu ─────────────────────────────────────────────────────────
pub fn run_interactive_menu() {
    let version = env!("CARGO_PKG_VERSION");

    loop {
        print_logo(version);

        let labels: Vec<&str> = MENU_ITEMS.iter().map(|(label, _)| *label).collect();

        let result = inquire::Select::new("What would you like to do?", labels).prompt();

        match result {
            Ok(selection) => {
                // Find the command key for the selected label
                let cmd = MENU_ITEMS
                    .iter()
                    .find(|(label, _)| *label == selection)
                    .map(|(_, cmd)| *cmd)
                    .unwrap_or("");

                // Category header selected — loop back (Pitfall 1)
                if cmd.is_empty() {
                    continue;
                }

                if cmd == "exit" {
                    output::info("Goodbye!");
                    std::process::exit(0);
                }

                // Execute the selected command and show any errors
                if let Err(e) = run_command(cmd) {
                    output::error(&format!("Error: {}", e));
                }

                // After command completes, pause briefly then loop back to menu
                output::print_line("");
            }
            Err(_) => {
                // User pressed Ctrl-C or Escape
                std::process::exit(0);
            }
        }
    }
}

// ─── Command dispatch ─────────────────────────────────────────────────────────
fn run_command(cmd: &str) -> Result<()> {
    // Load fresh config for each command so alias changes are picked up
    let loaded = crate::config::load_config();
    let (cfg, warnings) = match loaded {
        Ok(pair) => pair,
        Err(e) => {
            output::warn(&format!("Warning: could not load ezpm.toml: {}", e));
            (crate::config::EzpmConfig::default(), vec![])
        }
    };

    for w in &warnings {
        output::warn(w);
    }

    let src = cfg
        .paths
        .as_ref()
        .and_then(|p| p.src.as_deref())
        .unwrap_or("src");

    match cmd {
        "init" => crate::commands::init::run_init(),
        "install" => crate::commands::install::install_tools(src, cfg.aliases.as_ref()),
        "setup-wally-packages" => crate::commands::install::setup_wally_packages(src, cfg.aliases.as_ref()),
        "alias-menu" => crate::commands::alias::alias_menu(),
        "lint" => crate::commands::quality::lint(src),
        "format" => crate::commands::quality::format_code(src, false),
        "docs" => {
            let docs_enabled = cfg
                .display
                .as_ref()
                .and_then(|d| d.docs_enabled)
                .unwrap_or(false);
            crate::commands::quality::docs(docs_enabled)
        }
        "fix-requires" => {
            let aliases = cfg.aliases.unwrap_or_default();
            let result =
                crate::services::require_fixer::fix_requires(std::path::Path::new(src), &aliases, src)?;
            if result.files_changed == 0 {
                output::success(&format!(
                    "All requires up to date. 0 changes across {} files.",
                    result.total_files_scanned
                ));
            } else {
                let total_rewrites: usize = result.changes.iter().map(|c| c.rewrites.len()).sum();
                for file_change in &result.changes {
                    output::print_line(&format!("{}:", file_change.file.display()));
                    for rw in &file_change.rewrites {
                        output::print_line(&format!("  {} -> {}", rw.old, rw.new));
                    }
                }
                output::print_line("");
                output::success(&format!(
                    "Fixed {} requires across {} files",
                    total_rewrites, result.files_changed
                ));
            }
            Ok(())
        }
        "serve" => {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .context("failed to build tokio runtime")?;
            rt.block_on(crate::commands::serve::run(Some(cfg), None))
        }
        _ => {
            output::warn(&format!("Unknown command: {}", cmd));
            Ok(())
        }
    }
}
