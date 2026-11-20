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

    /// Print one JSON document on stdout instead of text (one event per line for `serve`)
    #[arg(long, global = true)]
    pub json: bool,

    /// Control color output
    #[arg(long, value_enum, global = true, default_value_t = ColorArg::Auto)]
    pub color: ColorArg,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Start the development server
    #[command(
        long_about = "Start require conversion, file watching, sourcemap generation, and Rojo serve."
    )]
    Serve {
        /// Override the Rojo serve port for this session
        #[arg(long, short = 'p')]
        port: Option<u16>,
    },

    /// Initialize a new project or adopt an existing Rojo project
    Init {
        /// Show the files ezpm would create without modifying the project
        #[arg(long)]
        dry_run: bool,

        /// Don't add ezpm's rules for coding agents to AGENTS.md
        #[arg(long, conflicts_with = "agent_rules")]
        no_agent_rules: bool,

        /// Only add the coding-agent rules (AGENTS.md, CLAUDE.md) to an existing project
        #[arg(long)]
        agent_rules: bool,

        /// Also register `ezpm hook stop` as a Claude Code Stop hook in .claude/settings.json
        #[arg(long)]
        agent_hook: bool,
    },

    /// Install tools and packages
    Install,

    /// Run linting checks
    Lint,

    /// Format source code
    Format {
        /// Exit non-zero if files are unformatted, without writing changes
        #[arg(long)]
        check: bool,
    },

    /// Fix require paths in source files
    #[command(name = "fix-requires")]
    FixRequires {
        /// List the rewrites without writing them
        #[arg(long, conflicts_with = "check")]
        dry_run: bool,

        /// Write nothing and exit 1 if any require would be rewritten (for CI)
        #[arg(long)]
        check: bool,

        /// Only report on these files (with --check or --dry-run)
        paths: Vec<String>,
    },

    /// Set up Wally packages
    #[command(name = "setup-wally-packages")]
    SetupWallyPackages,

    /// Analyze dependencies: detect circular requires, validate architecture rules, find unused modules
    Check {
        /// Only report findings about these files
        paths: Vec<String>,
    },

    /// Build a place or model with Rojo (arguments are passed to `rojo build`)
    #[command(
        long_about = "Sync .luaurc from ezpm.toml, then run `rojo build <project> <ARGS>`. Example: ezpm build -o game.rbxl"
    )]
    Build {
        #[arg(
            trailing_var_arg = true,
            allow_hyphen_values = true,
            value_name = "ROJO_ARGS"
        )]
        args: Vec<String>,
    },

    /// Write a Rojo sourcemap (arguments are passed to `rojo sourcemap`)
    #[command(
        long_about = "Sync .luaurc from ezpm.toml, then run `rojo sourcemap <project> <ARGS>`. Example: ezpm sourcemap -o sourcemap.json"
    )]
    Sourcemap {
        #[arg(
            trailing_var_arg = true,
            allow_hyphen_values = true,
            value_name = "ROJO_ARGS"
        )]
        args: Vec<String>,
    },

    /// Print where files land in the game and how to require them
    Where {
        #[arg(required = true)]
        paths: Vec<String>,
    },

    /// Explain a diagnostic code, such as `check.cycle`
    Explain {
        #[arg(required_unless_present = "list")]
        code: Option<String>,

        /// List every diagnostic code
        #[arg(long)]
        list: bool,
    },

    /// Hooks for coding agents
    Hook {
        #[command(subcommand)]
        hook: HookCommands,
    },

    /// Manage path aliases
    Alias {
        #[command(subcommand)]
        subcommand: Option<AliasCommands>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum HookFormat {
    /// Claude Code, Codex and Copilot: `{"decision":"block","reason":...}`
    #[default]
    Claude,
    /// Cursor: `{"followup_message":...}`
    Cursor,
}

#[derive(Debug, Subcommand)]
pub enum HookCommands {
    /// Report new problems in changed files when an agent ends its turn
    Stop {
        #[arg(long, value_enum, default_value_t = HookFormat::Claude)]
        format: HookFormat,
    },
}

#[derive(Debug, Subcommand)]
pub enum AliasCommands {
    /// Add an alias (prompts for anything not given)
    Add {
        /// Alias name, such as Shared (a leading @ is ignored)
        name: Option<String>,

        /// Folder the alias points at, such as src/shared/
        path: Option<String>,

        /// Create the folder if it doesn't exist
        #[arg(long)]
        create_dir: bool,
    },

    /// Remove aliases (prompts when no name is given)
    Remove { names: Vec<String> },

    /// List all aliases
    List,

    /// Sync aliases from ezpm.toml
    Sync,
}
