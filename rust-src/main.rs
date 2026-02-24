use clap::Parser;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use ezpm::{
    cli::{AliasCommands, Cli, Commands},
    commands::{alias, init, install, quality},
    config,
    services::{require_fixer, version},
};

// ─── Version check ────────────────────────────────────────────────────────────

/// Fetch the latest release tag from GitHub. Returns None on any network or
/// parse error so the version check is always non-fatal (Pitfall 6 from
/// RESEARCH.md — never block for long; uses background thread with 2s timeout).
///
/// ureq v3 changed `read_to_string` to take no arguments and return the string
/// directly (vs. ureq v2 which wrote into a &mut String).
fn fetch_latest_version() -> Option<String> {
    let body = ureq::get("https://api.github.com/repos/Breezy1214/ezpm/releases/latest")
        .header("Accept", "application/vnd.github.v3+json")
        .header(
            "User-Agent",
            &format!("ezpm/{}", env!("CARGO_PKG_VERSION")),
        )
        .call()
        .ok()?
        .body_mut()
        .read_to_string()
        .ok()?;

    let json: serde_json::Value = serde_json::from_str(&body).ok()?;
    json.get("tag_name")?.as_str().map(|s| s.to_string())
}

/// Print the version footer to stderr if a newer version is available.
/// Uses `recv_timeout` so we never block longer than 2 seconds (CLI-05).
/// stderr is intentional: the footer must not corrupt stdout piping.
fn print_version_footer(rx: &mpsc::Receiver<Option<String>>, current_ver: &str) {
    if let Ok(Some(latest)) = rx.recv_timeout(Duration::from_secs(2)) {
        if version::is_newer(current_ver, &latest) {
            eprintln!();
            eprintln!(
                "Update available: v{} -> {} — run rokit update ezpm",
                current_ver, latest
            );
        }
    }
}

// ─── Entry point ──────────────────────────────────────────────────────────────

fn main() {
    let cli = Cli::parse();

    // Load config at startup; print any warnings to stderr
    let loaded_config = match config::load_config() {
        Ok((cfg, warnings)) => {
            for w in &warnings {
                eprintln!("{}", w);
            }
            Some(cfg)
        }
        Err(e) => {
            eprintln!("Warning: could not load ezpm.toml: {}", e);
            None
        }
    };

    let current_ver = env!("CARGO_PKG_VERSION");

    // ── Background version check (CLI-05) ────────────────────────────────────
    // Disabled by EZPM_NO_UPDATE_CHECK=1 env var or check_updates=false in config.
    let check_disabled = std::env::var("EZPM_NO_UPDATE_CHECK").is_ok()
        || loaded_config
            .as_ref()
            .and_then(|c| c.display.as_ref())
            .and_then(|d| d.check_updates)
            .map(|v| !v)
            .unwrap_or(false);

    let (tx, rx) = mpsc::channel::<Option<String>>();

    if !check_disabled {
        thread::spawn(move || {
            let _ = tx.send(fetch_latest_version());
        });
    }

    // ── Command dispatch (CLI-02) ─────────────────────────────────────────────
    let src = loaded_config
        .as_ref()
        .and_then(|c| c.paths.as_ref())
        .and_then(|p| p.src.as_deref())
        .unwrap_or("src")
        .to_string();

    let result = match cli.command {
        None => {
            // No subcommand — show interactive menu (CLI-01)
            ezpm::menu::run_interactive_menu();
            Ok(())
        }
        Some(Commands::Init) => init::run_init(),
        Some(Commands::Install) => install::install_tools(&src),
        Some(Commands::SetupWallyPackages) => install::setup_wally_packages(&src),
        Some(Commands::Lint) => quality::lint(&src),
        Some(Commands::Format) => quality::format_code(&src),
        Some(Commands::Docs) => {
            let docs_enabled = loaded_config
                .as_ref()
                .and_then(|c| c.display.as_ref())
                .and_then(|d| d.docs_enabled)
                .unwrap_or(false);
            quality::docs(docs_enabled)
        }
        Some(Commands::FixRequires) => {
            let cfg = loaded_config.unwrap_or_default();
            let aliases = cfg.aliases.unwrap_or_default();
            let src_prefix = cfg
                .paths
                .as_ref()
                .and_then(|p| p.src.as_deref())
                .unwrap_or("src");
            let root_dir = match std::env::current_dir() {
                Ok(dir) => dir,
                Err(e) => {
                    eprintln!("Error: could not determine current directory: {}", e);
                    std::process::exit(1);
                }
            };
            match require_fixer::fix_requires(&root_dir, &aliases, src_prefix) {
                Ok(result) => {
                    if result.files_changed == 0 {
                        println!(
                            "All requires up to date. 0 changes across {} files.",
                            result.total_files_scanned
                        );
                    } else {
                        let total_rewrites: usize =
                            result.changes.iter().map(|c| c.rewrites.len()).sum();
                        for file_change in &result.changes {
                            println!("{}:", file_change.file.display());
                            for rewrite in &file_change.rewrites {
                                println!("  {} -> {}", rewrite.old, rewrite.new);
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
                Err(e) => Err(e),
            }
        }
        Some(Commands::Alias { subcommand }) => match subcommand {
            Some(AliasCommands::Add) => alias::alias_add(),
            Some(AliasCommands::Remove) => alias::alias_remove(),
            Some(AliasCommands::List) => {
                let aliases = loaded_config.and_then(|c| c.aliases);
                alias::alias_list(&aliases)
            }
            Some(AliasCommands::Sync) => alias::alias_sync(),
            None => {
                println!("Usage: ezpm alias <add|remove|list|sync>");
                println!();
                println!("Commands:");
                println!("  add     Add a new alias");
                println!("  remove  Remove an existing alias");
                println!("  list    List all aliases");
                println!("  sync    Sync aliases from ezpm.toml");
                Ok(())
            }
        },
        Some(Commands::Serve) => {
            println!(
                "serve is coming in a future update. Current version: {}",
                current_ver
            );
            Ok(())
        }
    };

    // ── Error handling ────────────────────────────────────────────────────────
    if let Err(e) = result {
        eprintln!("Error: {}", e);
        // Print version check footer even on error
        if !check_disabled {
            print_version_footer(&rx, current_ver);
        }
        std::process::exit(1);
    }

    // ── Version check footer (subtle, on stderr) ──────────────────────────────
    if !check_disabled {
        print_version_footer(&rx, current_ver);
    }
}
