use std::sync::OnceLock;
use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};
use owo_colors::OwoColorize;
use owo_colors::Stream;

pub struct OutputConfig {
    pub verbose: bool,
    pub quiet: bool,
    pub color: ColorChoice,
}

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
        ColorChoice::Auto => {}
    }
    let _ = OUTPUT_CONFIG.set(OutputConfig {
        verbose,
        quiet,
        color,
    });
}

fn config() -> &'static OutputConfig {
    OUTPUT_CONFIG
        .get()
        .expect("output::init must be called before any output function")
}

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

pub fn error(msg: &str) {
    eprintln!(
        "{} {}",
        "\u{2717}".if_supports_color(Stream::Stderr, |t| t.red()),
        msg
    );
}

pub fn error_block(error: &str, context: &str, fix: Option<&str>) {
    use std::fmt::Write as _;
    let mut label = String::new();

    label.clear();
    let _ = write!(
        label,
        "{}",
        "Error:".if_supports_color(Stream::Stderr, |t| t.red())
    );
    eprintln!("{} {}", label, error);

    label.clear();
    let _ = write!(
        label,
        "{}",
        "Context:".if_supports_color(Stream::Stderr, |t| t.yellow())
    );
    eprintln!("{} {}", label, context);

    if let Some(fix_msg) = fix {
        label.clear();
        let _ = write!(
            label,
            "{}",
            "Fix:".if_supports_color(Stream::Stderr, |t| t.green())
        );
        eprintln!("{} {}", label, fix_msg);
    }
}

pub fn hint(msg: &str) {
    if config().quiet {
        return;
    }
    eprintln!(
        "  hint: {}",
        msg.if_supports_color(Stream::Stderr, |t| t.cyan())
    );
}

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

pub fn verbose_line(msg: &str) {
    if !config().verbose {
        return;
    }
    println!("{}", msg.if_supports_color(Stream::Stdout, |t| t.dimmed()));
}

pub fn is_verbose() -> bool {
    config().verbose
}

pub fn print_line(msg: &str) {
    if config().quiet {
        return;
    }
    println!("{}", msg);
}

pub fn print_stderr(msg: &str) {
    if config().quiet {
        return;
    }
    eprintln!("{}", msg);
}

pub fn start_spinner(msg: &str) -> ProgressBar {
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
