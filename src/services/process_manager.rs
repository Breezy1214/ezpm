use std::collections::HashMap;
use std::process::Stdio;
#[cfg(unix)]
use std::time::Duration;

use anyhow::Result;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::output;

#[derive(Debug)]
pub enum ProcessEvent {
    Exited { name: String, code: Option<i32> },
}

#[cfg(windows)]
pub(crate) fn taskkill_tree(pid: u32) -> Result<()> {
    let output = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdin(Stdio::null())
        .output()?;
    anyhow::ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(())
}

struct ManagedProcess {
    pgid: u32,
    stop: oneshot::Sender<()>,
    monitor: JoinHandle<()>,
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
        self.kill(name).await;

        let mut command = tokio::process::Command::new(cmd);
        command
            .args(args)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .kill_on_drop(true);

        #[cfg(unix)]
        command.process_group(0);

        let child = command.spawn()?;
        let pgid = child
            .id()
            .ok_or_else(|| anyhow::anyhow!("{name} exited before its PID could be read"))?;

        output::verbose_line(&format!("Spawned {} (PID {})", name, pgid));

        let (stop, stop_rx) = oneshot::channel();
        let monitor = tokio::spawn(monitor(
            name.to_string(),
            pgid,
            child,
            stop_rx,
            self.status_tx.clone(),
        ));
        self.processes.insert(
            name.to_string(),
            ManagedProcess {
                pgid,
                stop,
                monitor,
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
        if let Some(proc) = self.processes.remove(name) {
            output::verbose_line(&format!("Stopping {} (PGID {})...", name, proc.pgid));
            let _ = proc.stop.send(());
            let _ = proc.monitor.await;
        }
    }
}

async fn monitor(
    name: String,
    pgid: u32,
    mut child: tokio::process::Child,
    stop: oneshot::Receiver<()>,
    status_tx: mpsc::Sender<ProcessEvent>,
) {
    tokio::select! {
        status = child.wait() => {
            // Anything the leader left running in its group would hold the port.
            #[cfg(unix)]
            let _ = nix::sys::signal::killpg(
                nix::unistd::Pid::from_raw(pgid as i32),
                nix::sys::signal::Signal::SIGKILL,
            );
            let code = status.ok().and_then(|status| status.code());
            let _ = status_tx.try_send(ProcessEvent::Exited { name, code });
        }
        _ = stop => shutdown(&name, pgid, &mut child).await,
    }
}

async fn shutdown(name: &str, pgid: u32, child: &mut tokio::process::Child) {
    #[cfg(unix)]
    {
        use nix::sys::signal::{killpg, Signal};
        use nix::unistd::Pid;

        let group = Pid::from_raw(pgid as i32);
        let _ = killpg(group, Signal::SIGTERM);

        if tokio::time::timeout(Duration::from_secs(2), child.wait())
            .await
            .is_err()
        {
            output::verbose_line(&format!(
                "{} did not exit within grace period — sending SIGKILL",
                name
            ));
            let _ = killpg(group, Signal::SIGKILL);
            let _ = child.wait().await;
        }
    }

    #[cfg(windows)]
    {
        let _ = name;
        if taskkill_tree(pgid).is_err() {
            let _ = child.start_kill();
        }
        let _ = child.wait().await;
    }
}

impl Default for ProcessManager {
    fn default() -> Self {
        ProcessManager::new().0
    }
}

impl Drop for ProcessManager {
    fn drop(&mut self) {
        if self.processes.is_empty() {
            return;
        }
        output::verbose_line(
            "ProcessManager dropped with active processes — use kill_all() for clean shutdown",
        );
        for proc in self.processes.values() {
            #[cfg(unix)]
            {
                use nix::sys::signal::{killpg, Signal};
                let _ = killpg(
                    nix::unistd::Pid::from_raw(proc.pgid as i32),
                    Signal::SIGTERM,
                );
            }
            #[cfg(windows)]
            let _ = taskkill_tree(proc.pgid);
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reports_unexpected_exit() {
        let (mut pm, mut rx) = ProcessManager::new();
        pm.spawn("short", "sh", &["-c", "exit 3"]).await.unwrap();

        let event = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("exit must be reported")
            .expect("channel open");
        let ProcessEvent::Exited { name, code } = event;
        assert_eq!(name, "short");
        assert_eq!(code, Some(3));
    }

    #[tokio::test]
    async fn deliberate_kill_is_silent_and_reaps_the_child() {
        let (mut pm, mut rx) = ProcessManager::new();
        pm.spawn("long", "sleep", &["30"]).await.unwrap();

        tokio::time::timeout(Duration::from_secs(5), pm.kill("long"))
            .await
            .expect("kill must finish within the grace period");

        assert!(
            rx.try_recv().is_err(),
            "a requested stop is not an exit event"
        );
        assert!(pm.processes.is_empty());
    }
}
