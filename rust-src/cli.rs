use clap::{Parser, Subcommand};

/// Roblox project manager
#[derive(Debug, Parser)]
#[command(name = "ezpm", version, about = "Roblox project manager")]
pub struct Cli {
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
