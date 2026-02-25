use clap::Parser;
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use ezpm::{
    cli::{AliasCommands, Cli, ColorArg, Commands},
    commands::{alias, init, install, quality, serve},
    config,
    output,
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
            output::print_stderr("");
            output::info(&format!(
                "Update available: v{} -> {} \u{2014} run rokit update ezpm",
                current_ver, latest
            ));
        }
    }
}

// ─── Entry point ──────────────────────────────────────────────────────────────

fn main() {
    let cli = Cli::parse();

    // Initialize output module immediately after parsing — must happen before
    // any output function is called, including config-load warnings (Pitfall 1).
    output::init(
        cli.verbose,
        cli.quiet,
        match cli.color {
            ColorArg::Auto => output::ColorChoice::Auto,
            ColorArg::Always => output::ColorChoice::Always,
            ColorArg::Never => output::ColorChoice::Never,
        },
    );

    // Load config at startup; print any warnings to stderr
    let loaded_config = match config::load_config() {
        Ok((cfg, warnings)) => {
            for w in &warnings {
                output::warn(w);
            }
            Some(cfg)
        }
        Err(e) => {
            output::warn(&format!("Could not load ezpm.toml: {}", e));
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
        Some(Commands::SetupWallyPackages) => {
            let aliases = loaded_config.as_ref().and_then(|c| c.aliases.as_ref());
            install::setup_wally_packages(&src, aliases)
        }
        Some(Commands::Lint) => quality::lint(&src),
        Some(Commands::Format { check }) => quality::format_code(&src, check),
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
            match require_fixer::fix_requires(Path::new(src_prefix), &aliases, src_prefix) {
                Ok(result) => {
                    if result.files_changed == 0 {
                        output::success(&format!(
                            "All requires up to date. 0 changes across {} files.",
                            result.total_files_scanned
                        ));
                    } else {
                        let total_rewrites: usize =
                            result.changes.iter().map(|c| c.rewrites.len()).sum();
                        for file_change in &result.changes {
                            output::print_line(&format!("{}:", file_change.file.display()));
                            for rewrite in &file_change.rewrites {
                                output::print_line(&format!("  {} -> {}", rewrite.old, rewrite.new));
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
            None => alias::alias_menu()
        },
        Some(Commands::Serve { port }) => {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("failed to build tokio runtime");
            rt.block_on(serve::run(loaded_config, port))
        }
    };

    // ── Error handling ────────────────────────────────────────────────────────
    if let Err(e) = result {
        output::error(&format!("{}", e));
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
