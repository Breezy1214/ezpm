use clap::{Parser, Subcommand, ValueEnum};

/// Color output control — maps to output::ColorChoice at startup.
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum ColorArg {
    /// Auto-detect based on TTY, NO_COLOR, FORCE_COLOR, and CI env vars (default).
    #[default]
    Auto,
    /// Force ANSI color output even when piped.
    Always,
    /// Force plain output even in a TTY.
    Never,
}

/// Roblox project manager
#[derive(Debug, Parser)]
#[command(name = "ezpm", version, about = "Roblox project manager")]
pub struct Cli {
    /// Show detailed step-by-step output
    #[arg(long, global = true)]
    pub verbose: bool,

    /// Suppress all non-error output
    #[arg(long, global = true)]
    pub quiet: bool,

    /// Control color output
    #[arg(long, value_enum, global = true, default_value_t = ColorArg::Auto)]
    pub color: ColorArg,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Start the development server
    Serve,

    /// Initialize a new project
    Init,

    /// Install tools and packages
    Install,

    /// Run linting checks
    Lint,

    /// Format source code
    Format,

    /// Open documentation server
    Docs,

    /// Fix require paths in source files
    #[command(name = "fix-requires")]
    FixRequires,

    /// Set up Wally packages
    #[command(name = "setup-wally-packages")]
    SetupWallyPackages,

    /// Manage path aliases
    Alias {
        #[command(subcommand)]
        subcommand: Option<AliasCommands>,
    },
}

#[derive(Debug, Subcommand)]
pub enum AliasCommands {
    /// Add a new alias
    Add,

    /// Remove an existing alias
    Remove,

    /// List all aliases
    List,

    /// Sync aliases from ezpm.toml
    Sync,
}
