use clap::Parser;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use ezpm::{
    cli::{AliasCommands, Cli, ColorArg, Commands},
    commands::{alias, check, fix_requires, init, install, quality, serve},
    config, output,
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
    let is_json_output = matches!(&cli.command, Some(Commands::Check { json: true }));

    let loaded_config_result = config::load_config();

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
        match cli.color {
            ColorArg::Auto => output::ColorChoice::Auto,
            ColorArg::Always => output::ColorChoice::Always,
            ColorArg::Never => output::ColorChoice::Never,
        },
    );

    let loaded_config = match loaded_config_result {
        Ok((cfg, warnings)) => {
            for w in &warnings {
                output::warn(w);
            }
            Some(cfg)
        }
        Err(e) if is_init || is_interactive_menu => {
            output::warn(&format!("Could not load ezpm.toml: {}", e));
            None
        }
        Err(e) => {
            output::error(&format!("Could not load ezpm.toml: {}", e));
            std::process::exit(1);
        }
    };

    let current_ver = env!("CARGO_PKG_VERSION");

    let check_disabled = effective_quiet
        || is_json_output
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

    let src = loaded_config
        .as_ref()
        .map_or_else(|| "src".to_string(), config::EzpmConfig::src_dir);

    let result = match cli.command {
        None => {
            ezpm::menu::run_interactive_menu(startup_update_notice.as_deref());
            Ok(())
        }
        Some(Commands::Init { dry_run }) => init::run_init(dry_run),
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
        Some(Commands::FixRequires) => {
            let cfg = loaded_config.unwrap_or_default();
            fix_requires::run(&cfg)
        }
        Some(Commands::Check { json }) => check::run_check(loaded_config.as_ref(), json),
        Some(Commands::Alias { subcommand }) => match subcommand {
            Some(AliasCommands::Add) => alias::alias_add(),
            Some(AliasCommands::Remove) => alias::alias_remove(),
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
            let result = rt.block_on(serve::run(loaded_config, port));
            // Don't wait on a background selene download after serve has stopped.
            rt.shutdown_timeout(std::time::Duration::from_secs(1));
            result
        }
    };

    if let Err(e) = result {
        output::error(&format!("{}", e));
        if !check_disabled && !update_check_already_handled {
            if let Some(notice) = get_update_notice(&rx, current_ver) {
                print_update_notice(&notice);
            }
        }
        std::process::exit(1);
    }

    if !check_disabled && !update_check_already_handled {
        if let Some(notice) = get_update_notice(&rx, current_ver) {
            print_update_notice(&notice);
        }
    }
}
