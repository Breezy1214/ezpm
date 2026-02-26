//! ProcessManager — spawn, track, and gracefully terminate child processes.

use std::collections::HashMap;
use std::process::Stdio;
#[cfg(unix)]
use std::time::Duration;

use anyhow::Result;
use tokio::sync::mpsc;

use crate::output;

// ─── Public types ─────────────────────────────────────────────────────────────

/// Lifecycle events emitted by `ProcessManager` to callers via mpsc channel.
#[derive(Debug)]
pub enum ProcessEvent {
    /// Emitted immediately after a process is successfully spawned.
    Started { name: String, pid: u32 },
    /// Emitted when a process exits with exit code 0.
    Exited { name: String, code: Option<i32> },
    /// Emitted when a process exits with a non-zero code, is killed, or its
    /// exit status cannot be determined.
    Crashed { name: String, code: Option<i32> },
}

// ─── Internal types ────────────────────────────────────────────────────────────

struct ManagedProcess {
    name: String,
    /// Process group ID — equals the child's PID because we spawn with
    /// `process_group(0)`.
    pgid: u32,
    child: tokio::process::Child,
}

// ─── ProcessManager ────────────────────────────────────────────────────────────

/// Manages a set of named child processes with graceful shutdown support.
///
/// See module-level documentation for usage.
pub struct ProcessManager {
    processes: HashMap<String, ManagedProcess>,
    status_tx: mpsc::Sender<ProcessEvent>,
}

impl ProcessManager {
    /// Create a new `ProcessManager`.
    ///
    /// Returns the manager and the receiver end of the lifecycle event channel.
    /// The caller owns the receiver and should `recv().await` on it inside a
    /// `tokio::select!` loop to observe process lifecycle events.
    pub fn new() -> (Self, mpsc::Receiver<ProcessEvent>) {
        let (status_tx, status_rx) = mpsc::channel::<ProcessEvent>(32);
        let manager = ProcessManager {
            processes: HashMap::new(),
            status_tx,
        };
        (manager, status_rx)
    }

    /// Spawn a child process under the given `name`.
    pub async fn spawn(&mut self, name: &str, cmd: &str, args: &[&str]) -> Result<()> {
        let mut command = tokio::process::Command::new(cmd);
        command
            .args(args)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        // Unix: spawn in own process group so PID == PGID.
        #[cfg(unix)]
        command.process_group(0);

        let child = command.spawn()?;

        let pgid = child.id().expect("freshly spawned child must have a PID");

        output::verbose_line(&format!("Spawned {} (PID {})", name, pgid));

        // Notify caller.
        let _ = self.status_tx.try_send(ProcessEvent::Started {
            name: name.to_string(),
            pid: pgid,
        });

        self.processes.insert(
            name.to_string(),
            ManagedProcess {
                name: name.to_string(),
                pgid,
                child,
            },
        );

        Ok(())
    }

    /// Gracefully terminate all managed processes.
    pub async fn kill_all(&mut self) {
        let names: Vec<String> = self.processes.keys().cloned().collect();

        for name in names {
            if let Some(mut proc) = self.processes.remove(&name) {
                self.shutdown_one(&mut proc).await;
            }
        }
    }

    /// Shut down a single `ManagedProcess` using the SIGTERM → SIGKILL protocol.
    async fn shutdown_one(&self, proc: &mut ManagedProcess) {
        output::verbose_line(&format!(
            "Stopping {} (PGID {})...",
            proc.name, proc.pgid
        ));

        #[cfg(unix)]
        {
            use nix::sys::signal::{killpg, Signal};
            use nix::unistd::Pid;

            let pgid = Pid::from_raw(proc.pgid as i32);

            // Step 1: SIGTERM to whole process group.
            let _ = killpg(pgid, Signal::SIGTERM);

            // Step 2: Wait up to 2 seconds for graceful exit.
            match tokio::time::timeout(Duration::from_secs(2), proc.child.wait()).await {
                Ok(Ok(status)) => {
                    let code = status.code();
                    let event = if status.success() {
                        ProcessEvent::Exited {
                            name: proc.name.clone(),
                            code,
                        }
                    } else {
                        ProcessEvent::Crashed {
                            name: proc.name.clone(),
                            code,
                        }
                    };
                    let _ = self.status_tx.try_send(event);
                }
                Ok(Err(_io_err)) => {
                    // wait() failed — treat as crash.
                    let _ = self.status_tx.try_send(ProcessEvent::Crashed {
                        name: proc.name.clone(),
                        code: None,
                    });
                }
                Err(_timeout) => {
                    // Step 3: Grace period expired — escalate to SIGKILL.
                    output::verbose_line(&format!(
                        "{} did not exit within grace period — sending SIGKILL",
                        proc.name
                    ));
                    let _ = killpg(pgid, Signal::SIGKILL);
                    // Reap the zombie (Pitfall 5 from RESEARCH.md).
                    let _ = proc.child.wait().await;
                    let _ = self.status_tx.try_send(ProcessEvent::Crashed {
                        name: proc.name.clone(),
                        code: None,
                    });
                }
            }
        }

        #[cfg(windows)]
        {
            // Windows has no process groups — best-effort direct child kill.
            let _ = proc.child.kill().await;
            let status = proc.child.wait().await.ok();
            let code = status.and_then(|s| s.code());
            let _ = self.status_tx.try_send(ProcessEvent::Crashed {
                name: proc.name.clone(),
                code,
            });
        }
    }
}

impl Default for ProcessManager {
    fn default() -> Self {
        ProcessManager::new().0
    }
}

impl Drop for ProcessManager {
    /// kill for any remaining processes.
    fn drop(&mut self) {
        if !self.processes.is_empty() {
            output::verbose_line(
                "ProcessManager dropped with active processes — use kill_all() for clean shutdown",
            );
            for proc in self.processes.values_mut() {
                // start_kill() is the synchronous, non-blocking variant.
                let _ = proc.child.start_kill();
            }
        }
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::time::Duration;

    #[cfg(unix)]
    use tokio::time::timeout;

    #[cfg(unix)]
    use super::*;

    #[cfg(unix)]
    fn init_output() {
        // ok() silently ignores double-init across tests (OnceLock pattern).
        output::init(false, false, output::ColorChoice::Auto);
    }

    /// Verify that a process can be spawned and that kill_all() terminates it
    /// and emits the expected lifecycle events.
    #[cfg(unix)]
    #[tokio::test]
    async fn test_spawn_and_kill_all() {
        init_output();

        let (mut manager, mut events) = ProcessManager::new();

        manager
            .spawn("sleeper", "sleep", &["30"])
            .await
            .expect("spawn must succeed");

        // Should receive a Started event.
        let started = timeout(Duration::from_secs(2), events.recv())
            .await
            .expect("timed out waiting for Started event")
            .expect("channel must not be closed");

        match started {
            ProcessEvent::Started { name, pid } => {
                assert_eq!(name, "sleeper");
                assert!(pid > 0, "PID must be positive");
            }
            other => panic!("expected Started, got {:?}", other),
        }

        // Graceful shutdown.
        manager.kill_all().await;

        // Should receive an Exited or Crashed event.
        let terminated = timeout(Duration::from_secs(5), events.recv())
            .await
            .expect("timed out waiting for termination event")
            .expect("channel must not be closed");

        match terminated {
            ProcessEvent::Exited { name, .. } | ProcessEvent::Crashed { name, .. } => {
                assert_eq!(name, "sleeper");
            }
            other => panic!("expected Exited or Crashed, got {:?}", other),
        }
    }

    /// Verify that kill_all() completes within the expected time window.
    ///
    /// SIGTERM + 2s grace + small overhead must fit within 5 seconds total.
    #[cfg(unix)]
    #[tokio::test]
    async fn test_kill_all_sigterm_grace() {
        init_output();

        let (mut manager, _events) = ProcessManager::new();

        manager
            .spawn("long_sleeper", "sleep", &["60"])
            .await
            .expect("spawn must succeed");

        // kill_all() must complete within 5 seconds (2s grace + margin).
        timeout(Duration::from_secs(5), manager.kill_all())
            .await
            .expect("kill_all() must complete within 5 seconds");
    }

    /// Verify that attempting to spawn a nonexistent command returns an error.
    #[cfg(unix)]
    #[tokio::test]
    async fn test_spawn_nonexistent_command() {
        init_output();

        let (mut manager, _events) = ProcessManager::new();

        let result = manager
            .spawn("bad", "nonexistent_cmd_xyz_abc_123", &[])
            .await;

        assert!(result.is_err(), "spawning a nonexistent command must fail");
    }

    /// Verify that kill_all() on an empty manager completes without error or panic.
    #[cfg(unix)]
    #[tokio::test]
    async fn test_kill_all_empty() {
        init_output();

        let (mut manager, _events) = ProcessManager::new();

        // Should complete immediately without any issues.
        manager.kill_all().await;
    }
}
