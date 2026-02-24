use clap::Parser;
use ezpm::{
    cli::{Cli, Commands},
    config,
    services::require_fixer,
};

fn main() {
    let cli = Cli::parse();

    // Load config at startup; print any warnings to stderr
    let loaded_config = match config::load_config() {
        Ok((cfg, warnings)) => {
            for warning in &warnings {
                eprintln!("{}", warning);
            }
            Some(cfg)
        }
        Err(e) => {
            eprintln!("Warning: could not load ezpm.toml: {}", e);
            None
        }
    };

    let version = env!("CARGO_PKG_VERSION");

    match cli.command {
        None => {
            ezpm::menu::run_interactive_menu();
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
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some(cmd) => {
            let name = match &cmd {
                Commands::Serve => "serve",
                Commands::Init => "init",
                Commands::Install => "install",
                Commands::Lint => "lint",
                Commands::Format => "format",
                Commands::Docs => "docs",
                Commands::FixRequires => unreachable!("handled above"),
                Commands::SetupWallyPackages => "setup-wally-packages",
                Commands::Alias { .. } => "alias",
            };
            println!(
                "{} is coming in a future update. Current version: {}",
                name, version
            );
        }
    }
}
