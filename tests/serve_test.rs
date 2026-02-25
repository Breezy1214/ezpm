mod common;

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Verify that `ezpm serve` starts up successfully:
///   1. Spawns the serve process in an isolated project directory.
///   2. Reads stdout line-by-line until the "Watching … for changes" ready
///      indicator appears (or a 30-second deadline is exceeded).
///   3. Asserts the ready line was observed.
///   4. Kills the process and waits for it to exit (SIGKILL exit is expected).
#[test]
fn serve_starts_and_shuts_down() {
    // Create an isolated project skeleton with all required files.
    let dir = common::create_project();

    let mut child = Command::new(common::ezpm_bin())
        .arg("serve")
        .arg("--port")
        .arg("44872") // Non-default port to avoid collisions with local Rojo
        .current_dir(dir.path())
        .env("EZPM_NO_UPDATE_CHECK", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn ezpm serve");

    // Wrap the piped stdout in a BufReader for line-by-line reading.
    let stdout = child.stdout.take().expect("child stdout was not piped");
    let reader = BufReader::new(stdout);

    let deadline = Instant::now() + Duration::from_secs(30);
    let mut ready = false;

    for line in reader.lines() {
        // Stop reading if the deadline is exceeded (prevents hanging forever).
        if Instant::now() >= deadline {
            break;
        }

        match line {
            Ok(text) => {
                // The serve command emits this line via output::info() after all
                // 8 startup steps complete (see rust-src/commands/serve.rs ~line 661).
                if text.contains("Watching") && text.contains("for changes") {
                    ready = true;
                    break;
                }
            }
            Err(_) => break, // EOF or pipe error — process exited
        }
    }

    // Kill the child process now that we have verified the ready line.
    // SIGKILL produces a non-zero exit status on Unix — that is expected.
    let _ = child.kill();
    let _ = child.wait();

    // Keep TempDir alive until after child.wait() — dropping it early would
    // delete the project files while the serve process might still be running.
    drop(dir);

    assert!(
        ready,
        "serve never became ready within 30 seconds — \
         ensure all Rokit tools (rojo, darklua, wally) are installed"
    );
}

/// Verify that `ezpm serve` exits with a non-zero code when no project config
/// exists. This tests the startup validation — serve should fail fast without
/// hanging if there is nothing to serve.
#[test]
fn serve_exits_nonzero_without_config() {
    // Bare TempDir with no ezpm.toml or project files.
    let dir = tempfile::TempDir::new().expect("TempDir::new failed");

    let output = Command::new(common::ezpm_bin())
        .arg("serve")
        .current_dir(dir.path())
        .env("EZPM_NO_UPDATE_CHECK", "1")
        .output()
        .expect("failed to spawn ezpm serve");

    common::assert_failure(&output);
}
