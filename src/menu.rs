use anyhow::{Context, Result};
use owo_colors::{OwoColorize, Stream};

use crate::output;

const MENU_ITEMS: &[(&str, &str, &str)] = &[
    ("init", "Create a new EZPM project", "init"),
    ("serve", "Start require watcher + Rojo", "serve"),
    (
        "fix-requires",
        "Resolve shorthand require paths",
        "fix-requires",
    ),
    ("install", "Install tools and packages", "install"),
    (
        "setup-wally-packages",
        "Clean + install + type Wally packages",
        "setup-wally-packages",
    ),
    (
        "alias",
        "Manage path aliases (add/remove/list/sync)",
        "alias-menu",
    ),
    ("lint", "Run Selene and StyLua checks", "lint"),
    ("format", "Format source with StyLua", "format"),
    ("docs", "Launch Moonwave docs server", "docs"),
    ("exit", "Exit", "exit"),
];

fn build_menu_options() -> Vec<(String, &'static str)> {
    let max_cmd_width = MENU_ITEMS
        .iter()
        .map(|(cmd, _, _)| cmd.chars().count())
        .max()
        .unwrap_or(0);

    MENU_ITEMS
        .iter()
        .map(|(cmd, desc, action)| {
            (
                format!("{cmd:<width$}   {desc}", width = max_cmd_width),
                *action,
            )
        })
        .collect()
}

fn print_logo(version: &str, update_notice: Option<&str>) {
    println!();
    println!(
        "{}",
        format!("EZPM v{}", version).if_supports_color(Stream::Stdout, |t| t.cyan())
    );
    if let Some(notice) = update_notice {
        println!(
            "{}",
            notice.if_supports_color(Stream::Stdout, |t| t.yellow())
        );
    }
    println!();
}

pub fn run_interactive_menu(update_notice: Option<&str>) {
    let version = env!("CARGO_PKG_VERSION");
    let mut menu_update_notice = update_notice;
    let options = build_menu_options();

    loop {
        print_logo(version, menu_update_notice);
        menu_update_notice = None;

        let labels: Vec<&str> = options.iter().map(|(label, _)| label.as_str()).collect();

        let result = inquire::Select::new("What would you like to do?", labels).prompt();

        match result {
            Ok(selection) => {
                let cmd = options
                    .iter()
                    .find(|(label, _)| label.as_str() == selection)
                    .map(|(_, cmd)| *cmd)
                    .unwrap_or("");

                if cmd.is_empty() {
                    continue;
                }

                if cmd == "exit" {
                    output::info("Goodbye!");
                    std::process::exit(0);
                }

                if let Err(e) = run_command(cmd) {
                    output::error(&format!("Error: {}", e));
                }

                output::print_line("");
            }
            Err(_) => {
                std::process::exit(0);
            }
        }
    }
}

fn run_command(cmd: &str) -> Result<()> {
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
        "init" => crate::commands::init::run_init(false),
        "install" => crate::commands::install::install_tools(src, cfg.aliases.as_ref()),
        "setup-wally-packages" => {
            crate::commands::install::setup_wally_packages(src, cfg.aliases.as_ref())
        }
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
        "fix-requires" => crate::commands::fix_requires::run(&cfg),
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
