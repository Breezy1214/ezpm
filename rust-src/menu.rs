use anyhow::Result;

// ─── Menu items ───────────────────────────────────────────────────────────────

/// Flat list of menu entries. Each entry is (display_label, command_key).
/// Category headers have an empty command_key — selecting one loops back to
/// the menu without executing any command (Pitfall 1 from RESEARCH.md).
const MENU_ITEMS: &[(&str, &str)] = &[
    ("init           Create a new EZPM project", "init"),
    ("serve          Start file watcher + DarkLua + Rojo", "serve"),
    ("fix-requires   Rewrite require paths to @alias notation", "fix-requires"),
    ("install        Install tools and packages", "install"),
    (
        "setup-wally-packages   Clean + install + type Wally packages",
        "setup-wally-packages",
    ),
    ("alias add      Add a new path alias", "alias-add"),
    ("alias remove   Remove path aliases", "alias-remove"),
    ("alias list     List all aliases", "alias-list"),
    ("alias sync     Sync aliases from ezpm.toml", "alias-sync"),
    ("lint           Run Selene and StyLua checks", "lint"),
    ("format         Format source with StyLua", "format"),
    ("docs           Launch Moonwave docs server", "docs"),
    ("exit", "exit"),
];

// ─── ASCII logo ───────────────────────────────────────────────────────────────
fn print_logo(version: &str) {
    println!();
    println!("  ███████╗███████╗██████╗ ███╗   ███╗");
    println!("  ██╔════╝╚══███╔╝██╔══██╗████╗ ████║");
    println!("  █████╗    ███╔╝ ██████╔╝██╔████╔██║");
    println!("  ██╔══╝   ███╔╝  ██╔═══╝ ██║╚██╔╝██║");
    println!("  ███████╗███████╗██║     ██║ ╚═╝ ██║");
    println!("  ╚══════╝╚══════╝╚═╝     ╚═╝     ╚═╝");
    println!("                              v{}", version);
    println!();
}

// ─── Interactive menu ─────────────────────────────────────────────────────────

/// Run the interactive arrow-key menu.
///
/// Displays the ASCII logo and a categorised list of commands. Selecting a
/// category header loops back to the menu. Selecting a command executes it
/// immediately and then returns to the menu. Ctrl-C / Escape exits cleanly.
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
                    println!("Goodbye!");
                    std::process::exit(0);
                }

                // Execute the selected command and show any errors
                if let Err(e) = run_command(cmd) {
                    eprintln!("Error: {}", e);
                }

                // After command completes, pause briefly then loop back to menu
                println!();
            }
            Err(_) => {
                // User pressed Ctrl-C or Escape
                std::process::exit(0);
            }
        }
    }
}

// ─── Command dispatch ─────────────────────────────────────────────────────────

/// Dispatch a menu command key to the appropriate handler.
///
/// Config is loaded fresh on each invocation so alias changes made during the
/// session are reflected without restarting.
fn run_command(cmd: &str) -> Result<()> {
    // Load fresh config for each command so alias changes are picked up
    let loaded = crate::config::load_config();
    let (cfg, warnings) = match loaded {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!("Warning: could not load ezpm.toml: {}", e);
            (crate::config::EzpmConfig::default(), vec![])
        }
    };

    for w in &warnings {
        eprintln!("{}", w);
    }

    let src = cfg
        .paths
        .as_ref()
        .and_then(|p| p.src.as_deref())
        .unwrap_or("src");

    match cmd {
        "init" => crate::commands::init::run_init(),
        "install" => crate::commands::install::install_tools(src),
        "setup-wally-packages" => crate::commands::install::setup_wally_packages(src),
        "alias-add" => crate::commands::alias::alias_add(),
        "alias-remove" => crate::commands::alias::alias_remove(),
        "alias-list" => crate::commands::alias::alias_list(&cfg.aliases),
        "alias-sync" => crate::commands::alias::alias_sync(),
        "lint" => crate::commands::quality::lint(src),
        "format" => crate::commands::quality::format_code(src),
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
            let root_dir = std::env::current_dir()?;
            let result =
                crate::services::require_fixer::fix_requires(&root_dir, &aliases, src)?;
            if result.files_changed == 0 {
                println!(
                    "All requires up to date. 0 changes across {} files.",
                    result.total_files_scanned
                );
            } else {
                let total_rewrites: usize = result.changes.iter().map(|c| c.rewrites.len()).sum();
                for file_change in &result.changes {
                    println!("{}:", file_change.file.display());
                    for rw in &file_change.rewrites {
                        println!("  {} -> {}", rw.old, rw.new);
                    }
                }
                println!();
                println!(
                    "Fixed {} requires across {} files",
                    total_rewrites, result.files_changed
                );
            }
            Ok(())
        }
        "serve" => {
            println!(
                "serve is coming in a future update. Current version: {}",
                env!("CARGO_PKG_VERSION")
            );
            Ok(())
        }
        _ => {
            eprintln!("Unknown command: {}", cmd);
            Ok(())
        }
    }
}
