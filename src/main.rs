use clap::Parser;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use ezpm::{
    cli::{AliasCommands, Cli, ColorArg, Commands, HookCommands},
    commands::{
        alias, check, explain, fix_requires, hook, init, install, quality, rojo, serve, where_cmd,
    },
    config,
    diagnostics::{self, Diagnostic},
    error::{ErrorKind, EzpmError},
    output,
    services::version,
};

fn fetch_latest_version() -> Option<String> {
    let body = ureq::get("https://api.github.com/repos/Breezy1214/ezpm/releases/latest")
        .header("Accept", "application/vnd.github.v3+json")
        .header("User-Agent", &format!("ezpm/{}", env!("CARGO_PKG_VERSION")))
        .call()
        .ok()?
        .body_mut()
        .read_to_string()
        .ok()?;

    let json: serde_json::Value = serde_json::from_str(&body).ok()?;
    json.get("tag_name")?.as_str().map(|s| s.to_string())
}

fn get_update_notice(rx: &mpsc::Receiver<Option<String>>, current_ver: &str) -> Option<String> {
    if let Ok(Some(latest)) = rx.recv_timeout(Duration::from_secs(2)) {
        if version::is_newer(current_ver, &latest) {
            return Some(format!(
                "Update available: v{} -> {} \u{2014} run rokit update ezpm",
                current_ver, latest
            ));
        }
    }

    None
}

fn print_update_notice(notice: &str) {
    output::print_stderr("");
    output::notice(notice);
}

fn main() {
    let cli = Cli::parse();
    let is_long_running_serve = matches!(&cli.command, Some(Commands::Serve { .. }));
    let is_interactive_menu = cli.command.is_none();
    let is_init = matches!(&cli.command, Some(Commands::Init { .. }));
    let skips_config = matches!(&cli.command, Some(Commands::Explain { .. }));

    let loaded_config_result = if skips_config {
        Ok((config::EzpmConfig::default(), Vec::new()))
    } else {
        config::load_config()
    };

    let logs_enabled = loaded_config_result
        .as_ref()
        .ok()
        .and_then(|(cfg, _)| cfg.display.as_ref())
        .and_then(|d| d.logs_enabled)
        .unwrap_or(true);

    let effective_quiet = cli.quiet || (!logs_enabled && !cli.verbose);

    output::init(
        cli.verbose,
        effective_quiet,
        cli.json,
        match cli.color {
            ColorArg::Auto => output::ColorChoice::Auto,
            ColorArg::Always => output::ColorChoice::Always,
            ColorArg::Never => output::ColorChoice::Never,
        },
    );

    let loaded_config = match loaded_config_result {
        Ok((cfg, warnings)) => {
            diagnostics::report(&warnings);
            Some(cfg)
        }
        Err(e) if is_init || is_interactive_menu => {
            output::warn(&format!("Could not load ezpm.toml: {}", e));
            None
        }
        Err(e) => {
            let diagnostic = Diagnostic::new("config.invalid", e.to_string()).at("ezpm.toml", None);
            if output::is_json() {
                output::document(&serde_json::json!({ "diagnostics": [diagnostic] }));
            } else {
                diagnostics::report(std::slice::from_ref(&diagnostic));
            }
            std::process::exit(1);
        }
    };

    let current_ver = env!("CARGO_PKG_VERSION");

    let check_disabled = effective_quiet
        || cli.json
        || std::env::var("EZPM_NO_UPDATE_CHECK").is_ok()
        || std::env::var_os("CI").is_some()
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

    let startup_update_notice = if (is_long_running_serve || is_interactive_menu) && !check_disabled
    {
        get_update_notice(&rx, current_ver)
    } else {
        None
    };

    let update_check_already_handled = is_long_running_serve || is_interactive_menu;

    if is_long_running_serve {
        if let Some(notice) = startup_update_notice.as_deref() {
            print_update_notice(notice);
        }
    }

    let result = run_command(cli.command, loaded_config, startup_update_notice.as_deref());
    let exit_code = report_result(result);

    if !check_disabled && !update_check_already_handled {
        if let Some(notice) = get_update_notice(&rx, current_ver) {
            print_update_notice(&notice);
        }
    }

    if exit_code != 0 {
        std::process::exit(exit_code);
    }
}

fn run_command(
    command: Option<Commands>,
    loaded_config: Option<config::EzpmConfig>,
    startup_update_notice: Option<&str>,
) -> anyhow::Result<()> {
    let src = loaded_config
        .as_ref()
        .map_or_else(|| "src".to_string(), config::EzpmConfig::src_dir);

    match command {
        None => {
            ezpm::menu::run_interactive_menu(startup_update_notice);
            Ok(())
        }
        Some(Commands::Init {
            dry_run,
            no_agent_rules,
            agent_rules,
            agent_hook,
        }) => init::run_init(init::InitOptions {
            dry_run,
            agent_rules: !no_agent_rules,
            agent_rules_only: agent_rules,
            agent_hook,
        }),
        Some(Commands::Install) => {
            let aliases = loaded_config.as_ref().and_then(|c| c.aliases.as_ref());
            install::install_tools(&src, aliases)
        }
        Some(Commands::SetupWallyPackages) => {
            let aliases = loaded_config.as_ref().and_then(|c| c.aliases.as_ref());
            install::setup_wally_packages(&src, aliases)
        }
        Some(Commands::Lint) => quality::lint(&src),
        Some(Commands::Format { check }) => quality::format_code(&src, check),
        Some(Commands::FixRequires {
            dry_run,
            check,
            paths,
        }) => {
            let mode = if check {
                fix_requires::Mode::Check
            } else if dry_run {
                fix_requires::Mode::DryRun
            } else {
                fix_requires::Mode::Write
            };
            fix_requires::run(&loaded_config.unwrap_or_default(), mode, &paths)
        }
        Some(Commands::Check { paths }) => check::run_check(loaded_config.as_ref(), &paths),
        Some(Commands::Build { args }) => rojo::run(
            &loaded_config.unwrap_or_default(),
            rojo::RojoCommand::Build,
            &args,
        ),
        Some(Commands::Sourcemap { args }) => rojo::run(
            &loaded_config.unwrap_or_default(),
            rojo::RojoCommand::Sourcemap,
            &args,
        ),
        Some(Commands::Where { paths }) => {
            where_cmd::run(&loaded_config.unwrap_or_default(), &paths)
        }
        Some(Commands::Explain { code, list }) => explain::run(code.as_deref(), list),
        Some(Commands::Hook {
            hook: HookCommands::Stop { format },
        }) => hook::stop(format),
        Some(Commands::Alias { subcommand }) => match subcommand {
            Some(AliasCommands::Add {
                name,
                path,
                create_dir,
            }) => alias::alias_add(name, path, create_dir),
            Some(AliasCommands::Remove { names }) => alias::alias_remove(&names),
            Some(AliasCommands::List) => {
                let aliases = loaded_config.and_then(|c| c.aliases);
                alias::alias_list(&aliases)
            }
            Some(AliasCommands::Sync) => alias::alias_sync(),
            None => alias::alias_menu(),
        },
        Some(Commands::Serve { port }) => {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("failed to build tokio runtime");
            rt.block_on(serve::run(loaded_config, port))
        }
    }
}

/// Prints a failed command's error (as text, or as a JSON error document) and returns the exit
/// code: 0 done, 1 project or tool errors, 2 bad usage, 130 interrupted.
fn report_result(result: anyhow::Result<()>) -> i32 {
    let Err(error) = result else {
        return 0;
    };

    if let Some(inquire::InquireError::OperationInterrupted) =
        error.downcast_ref::<inquire::InquireError>()
    {
        return 130;
    }

    let (kind, code, hint, reported) = match error.downcast_ref::<EzpmError>() {
        Some(ezpm_error) => (
            ezpm_error.kind_name(),
            ezpm_error.exit_code(),
            ezpm_error.hint.clone(),
            matches!(ezpm_error.kind, ErrorKind::Reported(_)),
        ),
        None => ("project", 1, None, false),
    };
    let message = error.to_string();

    if output::is_json() {
        if !reported {
            let mut body = serde_json::json!({ "kind": kind, "message": message });
            if let Some(hint) = hint {
                body["hint"] = hint.into();
            }
            output::document(&serde_json::json!({ "error": body }));
        }
    } else if !message.is_empty() {
        output::error(&message);
        if let Some(hint) = hint {
            output::hint(&hint);
        }
    }

    code
}
