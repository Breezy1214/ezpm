use std::collections::HashMap;
use std::process::Stdio;
#[cfg(unix)]
use std::time::Duration;

use anyhow::Result;
use tokio::sync::mpsc;

use crate::output;

#[derive(Debug)]
pub enum ProcessEvent {
    Started { name: String, pid: u32 },
    Exited { name: String, code: Option<i32> },
    Crashed { name: String, code: Option<i32> },
}

struct ManagedProcess {
    name: String,
    pgid: u32,
    child: tokio::process::Child,
}

pub struct ProcessManager {
    processes: HashMap<String, ManagedProcess>,
    status_tx: mpsc::Sender<ProcessEvent>,
}

impl ProcessManager {
    pub fn new() -> (Self, mpsc::Receiver<ProcessEvent>) {
        let (status_tx, status_rx) = mpsc::channel::<ProcessEvent>(32);
        let manager = ProcessManager {
            processes: HashMap::new(),
            status_tx,
        };
        (manager, status_rx)
    }

    pub async fn spawn(&mut self, name: &str, cmd: &str, args: &[&str]) -> Result<()> {
        let mut command = tokio::process::Command::new(cmd);
        command
            .args(args)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());

        #[cfg(unix)]
        command.process_group(0);

        let child = command.spawn()?;

        let pgid = child.id().expect("freshly spawned child must have a PID");

        output::verbose_line(&format!("Spawned {} (PID {})", name, pgid));

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

    pub async fn kill_all(&mut self) {
        let names: Vec<String> = self.processes.keys().cloned().collect();

        for name in names {
            self.kill(&name).await;
        }
    }

    pub async fn kill(&mut self, name: &str) {
        if let Some(mut proc) = self.processes.remove(name) {
            self.shutdown_one(&mut proc).await;
        }
    }

    async fn shutdown_one(&self, proc: &mut ManagedProcess) {
        output::verbose_line(&format!("Stopping {} (PGID {})...", proc.name, proc.pgid));

        #[cfg(unix)]
        {
            use nix::sys::signal::{killpg, Signal};
            use nix::unistd::Pid;

            let pgid = Pid::from_raw(proc.pgid as i32);

            let _ = killpg(pgid, Signal::SIGTERM);

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
                    let _ = self.status_tx.try_send(ProcessEvent::Crashed {
                        name: proc.name.clone(),
                        code: None,
                    });
                }
                Err(_timeout) => {
                    output::verbose_line(&format!(
                        "{} did not exit within grace period — sending SIGKILL",
                        proc.name
                    ));
                    let _ = killpg(pgid, Signal::SIGKILL);
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
    fn drop(&mut self) {
        if !self.processes.is_empty() {
            output::verbose_line(
                "ProcessManager dropped with active processes — use kill_all() for clean shutdown",
            );
            for proc in self.processes.values_mut() {
                let _ = proc.child.start_kill();
            }
        }
    }
}

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
        output::init(false, false, output::ColorChoice::Auto);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_spawn_and_kill_all() {
        init_output();

        let (mut manager, mut events) = ProcessManager::new();

        manager
            .spawn("sleeper", "sleep", &["30"])
            .await
            .expect("spawn must succeed");

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

        manager.kill_all().await;

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

    #[cfg(unix)]
    #[tokio::test]
    async fn test_kill_all_sigterm_grace() {
        init_output();

        let (mut manager, _events) = ProcessManager::new();

        manager
            .spawn("long_sleeper", "sleep", &["60"])
            .await
            .expect("spawn must succeed");

        timeout(Duration::from_secs(5), manager.kill_all())
            .await
            .expect("kill_all() must complete within 5 seconds");
    }

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

    #[cfg(unix)]
    #[tokio::test]
    async fn test_kill_all_empty() {
        init_output();

        let (mut manager, _events) = ProcessManager::new();

        manager.kill_all().await;
    }
}
