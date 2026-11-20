use std::sync::OnceLock;
use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};
use owo_colors::OwoColorize;
use owo_colors::Stream;

pub struct OutputConfig {
    pub verbose: bool,
    pub quiet: bool,
    pub json: bool,
    pub color: ColorChoice,
}

#[derive(Clone, Copy, PartialEq)]
pub enum ColorChoice {
    Auto,
    Always,
    Never,
}

macro_rules! out {
    ($($arg:tt)*) => {{
        use std::io::Write as _;
        let _ = writeln!(std::io::stdout(), $($arg)*);
    }};
}

macro_rules! err {
    ($($arg:tt)*) => {{
        use std::io::Write as _;
        let _ = writeln!(std::io::stderr(), $($arg)*);
    }};
}

static OUTPUT_CONFIG: OnceLock<OutputConfig> = OnceLock::new();

/// In JSON mode every human-readable line is suppressed, so stdout carries only JSON.
pub fn init(verbose: bool, quiet: bool, json: bool, color: ColorChoice) {
    match color {
        ColorChoice::Always => owo_colors::set_override(true),
        ColorChoice::Never => owo_colors::set_override(false),
        ColorChoice::Auto => {}
    }
    let _ = OUTPUT_CONFIG.set(OutputConfig {
        verbose: verbose && !json,
        quiet: quiet || json,
        json,
        color,
    });
}

#[cfg(not(test))]
fn config() -> &'static OutputConfig {
    OUTPUT_CONFIG
        .get()
        .expect("output::init must be called before any output function")
}

#[cfg(test)]
fn config() -> &'static OutputConfig {
    OUTPUT_CONFIG.get_or_init(|| OutputConfig {
        verbose: false,
        quiet: false,
        json: false,
        color: ColorChoice::Auto,
    })
}

pub fn success(msg: &str) {
    if config().quiet {
        return;
    }
    out!(
        "{} {}",
        "\u{2713}".if_supports_color(Stream::Stdout, |t| t.green()),
        msg
    );
}

pub fn error(msg: &str) {
    if config().json {
        return;
    }
    err!(
        "{} {}",
        "\u{2717}".if_supports_color(Stream::Stderr, |t| t.red()),
        msg
    );
}

pub fn error_block(error: &str, context: &str, fix: Option<&str>) {
    use std::fmt::Write as _;
    if config().json {
        return;
    }
    let mut label = String::new();

    label.clear();
    let _ = write!(
        label,
        "{}",
        "Error:".if_supports_color(Stream::Stderr, |t| t.red())
    );
    err!("{} {}", label, error);

    label.clear();
    let _ = write!(
        label,
        "{}",
        "Context:".if_supports_color(Stream::Stderr, |t| t.yellow())
    );
    err!("{} {}", label, context);

    if let Some(fix_msg) = fix {
        label.clear();
        let _ = write!(
            label,
            "{}",
            "Fix:".if_supports_color(Stream::Stderr, |t| t.green())
        );
        err!("{} {}", label, fix_msg);
    }
}

pub fn hint(msg: &str) {
    if config().quiet {
        return;
    }
    err!(
        "  hint: {}",
        msg.if_supports_color(Stream::Stderr, |t| t.cyan())
    );
}

pub fn info(msg: &str) {
    if config().quiet {
        return;
    }
    out!(
        "{} {}",
        "\u{25B8}".if_supports_color(Stream::Stdout, |t| t.cyan()),
        msg
    );
}

pub fn warn(msg: &str) {
    if config().quiet {
        return;
    }
    err!(
        "{} {}",
        "\u{26A0}".if_supports_color(Stream::Stderr, |t| t.yellow()),
        msg
    );
}

/// Diagnostics go to stderr so stdout stays parseable (e.g. `check --json --verbose`).
pub fn verbose_line(msg: &str) {
    if !config().verbose {
        return;
    }
    err!("{}", msg.if_supports_color(Stream::Stderr, |t| t.dimmed()));
}

pub fn notice(msg: &str) {
    if config().quiet {
        return;
    }
    err!(
        "{} {}",
        "\u{25B8}".if_supports_color(Stream::Stderr, |t| t.cyan()),
        msg
    );
}

pub fn is_verbose() -> bool {
    config().verbose
}

pub fn is_quiet() -> bool {
    config().quiet
}

pub fn is_json() -> bool {
    config().json
}

/// Prints one JSON document on one line of stdout.
pub fn document(value: &impl serde::Serialize) {
    match serde_json::to_string(value) {
        Ok(json) => out!("{}", json),
        Err(e) => err!("failed to serialize JSON output: {}", e),
    }
}

/// Prints one `serve --json` event: `{"event": kind, ...fields}` on one line of stdout.
pub fn event(kind: &str, fields: serde_json::Value) {
    let mut object = serde_json::Map::new();
    object.insert("event".into(), kind.into());
    if let serde_json::Value::Object(fields) = fields {
        object.extend(fields);
    }
    document(&serde_json::Value::Object(object));
}

pub fn format_duration(duration: Duration) -> String {
    if duration < Duration::from_millis(1) {
        "<1ms".to_string()
    } else {
        format!("{}ms", duration.as_millis())
    }
}

pub fn print_line(msg: &str) {
    if config().quiet {
        return;
    }
    out!("{}", msg);
}

pub fn print_stderr(msg: &str) {
    if config().quiet {
        return;
    }
    err!("{}", msg);
}

pub fn start_spinner(msg: &str) -> ProgressBar {
    if config().quiet {
        return ProgressBar::hidden();
    }
    let pb = ProgressBar::new_spinner();
    pb.enable_steady_tick(Duration::from_millis(80));
    pb.set_style(
        ProgressStyle::default_spinner()
            .tick_strings(&[
                "\u{280B}", "\u{2819}", "\u{2839}", "\u{2838}", "\u{283C}", "\u{2834}", "\u{2826}",
                "\u{2827}", "\u{2807}", "\u{280F}", "\u{2713}",
            ])
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    pb.set_message(msg.to_string());
    pb
}
