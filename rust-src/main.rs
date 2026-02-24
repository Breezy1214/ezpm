use clap::Parser;
use ezpm::{cli::{Cli, Commands}, config, menu};

fn main() {
    let cli = Cli::parse();

    // Load config at startup; print any warnings to stderr
    match config::load_config() {
        Ok((_, warnings)) => {
            for warning in &warnings {
                eprintln!("{}", warning);
            }
        }
        Err(e) => {
            eprintln!("Warning: could not load ezpm.toml: {}", e);
        }
    }

    let version = env!("CARGO_PKG_VERSION");

    match cli.command {
        None => {
            menu::run_interactive_menu();
        }
        Some(cmd) => {
            let name = match &cmd {
                Commands::Serve => "serve",
                Commands::Init => "init",
                Commands::Install => "install",
                Commands::Lint => "lint",
                Commands::Format => "format",
                Commands::Docs => "docs",
                Commands::FixRequires => "fix-requires",
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
