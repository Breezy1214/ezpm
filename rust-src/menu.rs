use inquire::Select;
use std::process;

const MENU_OPTIONS: &[&str] = &[
    "Serve",
    "Init",
    "Install",
    "Lint",
    "Format",
    "Docs",
    "Fix Requires",
    "Setup Wally Packages",
    "Alias",
    "Exit",
];

pub fn run_interactive_menu() {
    let version = env!("CARGO_PKG_VERSION");

    loop {
        let result = Select::new("What would you like to do?", MENU_OPTIONS.to_vec()).prompt();

        match result {
            Ok(selection) => {
                if selection == "Exit" {
                    process::exit(0);
                } else {
                    println!(
                        "{} is coming in a future update. Current version: {}",
                        selection, version
                    );
                }
            }
            Err(_) => {
                process::exit(0);
            }
        }
    }
}
