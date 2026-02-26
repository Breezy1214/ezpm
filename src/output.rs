//! Centralized output module for ezpm.

use std::sync::OnceLock;
use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};
use owo_colors::OwoColorize;
use owo_colors::Stream;

// ─── OutputConfig ─────────────────────────────────────────────────────────────

/// Global output configuration set once at startup via `output::init()`.
pub struct OutputConfig {
    pub verbose: bool,
    pub quiet: bool,
    pub color: ColorChoice,
}

/// Color output control, maps 1-to-1 from CLI --color flag.
#[derive(Clone, Copy, PartialEq)]
pub enum ColorChoice {
    Auto,
    Always,
    Never,
}

static OUTPUT_CONFIG: OnceLock<OutputConfig> = OnceLock::new();

pub fn init(verbose: bool, quiet: bool, color: ColorChoice) {
    match color {
        ColorChoice::Always => owo_colors::set_override(true),
        ColorChoice::Never => owo_colors::set_override(false),
        ColorChoice::Auto => {} // owo-colors handles NO_COLOR, TTY, CI automatically
    }
    let _ = OUTPUT_CONFIG.set(OutputConfig { verbose, quiet, color });
}

fn config() -> &'static OutputConfig {
    OUTPUT_CONFIG
        .get()
        .expect("output::init must be called before any output function")
}

// ─── Typed output functions ────────────────────────────────────────────────────

/// Print a success message to stdout.
/// Format: "✓ msg" with a green checkmark.
/// Suppressed by --quiet.
pub fn success(msg: &str) {
    if config().quiet {
        return;
    }
    println!(
        "{} {}",
        "\u{2713}".if_supports_color(Stream::Stdout, |t| t.green()),
        msg
    );
}

/// Print an error message to stderr.
/// Format: "✗ msg" with a red cross.
/// ALWAYS prints — not suppressed by --quiet. Errors must always be visible.
pub fn error(msg: &str) {
    eprintln!(
        "{} {}",
        "\u{2717}".if_supports_color(Stream::Stderr, |t| t.red()),
        msg
    );
}

/// Print a structured Error/Context/Fix block to stderr for command failures.
///
/// Prints three labeled sections:
///   - "Error:"   in red bold   — what went wrong
///   - "Context:" in yellow bold — why / where
///   - "Fix:"     in green bold  — how to resolve it (omitted if `fix` is None)
///
/// ALWAYS prints — not suppressed by --quiet. Same contract as error().
/// Automatically respects NO_COLOR, --color never, and CI detection via
/// if_supports_color(Stream::Stderr, ...).
pub fn error_block(error: &str, context: &str, fix: Option<&str>) {
    use std::fmt::Write as _;
    let mut label = String::new();

    label.clear();
    let _ = write!(label, "{}", "Error:".if_supports_color(Stream::Stderr, |t| t.red()));
    eprintln!("{} {}", label, error);

    label.clear();
    let _ = write!(label, "{}", "Context:".if_supports_color(Stream::Stderr, |t| t.yellow()));
    eprintln!("{} {}", label, context);

    if let Some(fix_msg) = fix {
        label.clear();
        let _ = write!(label, "{}", "Fix:".if_supports_color(Stream::Stderr, |t| t.green()));
        eprintln!("{} {}", label, fix_msg);
    }
}

/// Print an indented hint line to stderr, typically below an error.
/// Format: "  hint: msg" with cyan text.
/// Suppressed by --quiet.
pub fn hint(msg: &str) {
    if config().quiet {
        return;
    }
    eprintln!(
        "  hint: {}",
        msg.if_supports_color(Stream::Stderr, |t| t.cyan())
    );
}

/// Print an informational message to stdout.
/// Format: "▸ msg" with a cyan triangle.
/// Suppressed by --quiet.
pub fn info(msg: &str) {
    if config().quiet {
        return;
    }
    println!(
        "{} {}",
        "\u{25B8}".if_supports_color(Stream::Stdout, |t| t.cyan()),
        msg
    );
}

/// Print a warning message to stderr.
/// Format: "⚠ msg" with a yellow warning sign.
/// Suppressed by --quiet. (Warnings are caution-level, not error-level.)
pub fn warn(msg: &str) {
    if config().quiet {
        return;
    }
    eprintln!(
        "{} {}",
        "\u{26A0}".if_supports_color(Stream::Stderr, |t| t.yellow()),
        msg
    );
}

/// Print a verbose-only detail line to stdout in dimmed/gray.
/// Only prints when --verbose is active. Suppressed otherwise.
pub fn verbose_line(msg: &str) {
    if !config().verbose {
        return;
    }
    println!("{}", msg.if_supports_color(Stream::Stdout, |t| t.dimmed()));
}

/// Returns true if --verbose flag is active.
/// Used by command handlers to decide whether to use .status() (pass-through)
/// or .output() (capture) for subprocess calls.
pub fn is_verbose() -> bool {
    config().verbose
}

/// Print a neutral line to stdout. Plain println! replacement for rows that
/// are neither success/error/info/warn (e.g., alias table rows, help text).
/// Suppressed by --quiet.
pub fn print_line(msg: &str) {
    if config().quiet {
        return;
    }
    println!("{}", msg);
}

/// Print a neutral line to stderr. Plain eprintln! replacement for non-error
/// stderr output (e.g., version check footer, blank separator lines).
/// Suppressed by --quiet.
pub fn print_stderr(msg: &str) {
    if config().quiet {
        return;
    }
    eprintln!("{}", msg);
}

// ─── Spinner API ───────────────────────────────────────────────────────────────

pub fn start_spinner(msg: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.enable_steady_tick(Duration::from_millis(80));
    pb.set_style(
        ProgressStyle::default_spinner()
            .tick_strings(&[
                "\u{280B}", // ⠋
                "\u{2819}", // ⠙
                "\u{2839}", // ⠹
                "\u{2838}", // ⠸
                "\u{283C}", // ⠼
                "\u{2834}", // ⠴
                "\u{2826}", // ⠦
                "\u{2827}", // ⠧
                "\u{2807}", // ⠇
                "\u{280F}", // ⠏
                "\u{2713}", // ✓ — final frame shown briefly before clear
            ])
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    pb.set_message(msg.to_string());
    pb
}
