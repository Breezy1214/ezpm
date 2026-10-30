#[cfg(unix)]
use std::time::{Duration, Instant};

pub fn is_available(port: u16) -> bool {
    [[127, 0, 0, 1], [0, 0, 0, 0]]
        .into_iter()
        .all(|ip| std::net::TcpListener::bind(std::net::SocketAddr::from((ip, port))).is_ok())
}

pub fn next_available(after: u16) -> Option<u16> {
    (after.saturating_add(1)..=after.saturating_add(100)).find(|&p| is_available(p))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortOwner {
    pub pid: u32,
    pub name: String,
}

pub fn owners(port: u16) -> Vec<PortOwner> {
    std::process::Command::new("lsof")
        .args(["-nP", &format!("-iTCP:{port}"), "-sTCP:LISTEN", "-Fpc"])
        .output()
        .map(|out| parse_lsof(&String::from_utf8_lossy(&out.stdout)))
        .unwrap_or_default()
}

fn parse_lsof(output: &str) -> Vec<PortOwner> {
    let mut owners: Vec<PortOwner> = Vec::new();
    for line in output.lines() {
        if let Some(pid) = line.strip_prefix('p').and_then(|p| p.parse().ok()) {
            owners.push(PortOwner {
                pid,
                name: String::new(),
            });
        } else if let (Some(name), Some(owner)) = (line.strip_prefix('c'), owners.last_mut()) {
            owner.name = name.to_string();
        }
    }
    owners
}

#[cfg(unix)]
pub fn kill_owners(owners: &[PortOwner], port: u16) -> anyhow::Result<()> {
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;

    for owner in owners {
        kill(Pid::from_raw(owner.pid as i32), Signal::SIGTERM).map_err(|e| {
            anyhow::anyhow!("could not stop {} (PID {}): {}", owner.name, owner.pid, e)
        })?;
    }
    if wait_until_free(port, Duration::from_secs(3)) {
        return Ok(());
    }
    for owner in owners {
        let _ = kill(Pid::from_raw(owner.pid as i32), Signal::SIGKILL);
    }
    if wait_until_free(port, Duration::from_secs(2)) {
        return Ok(());
    }
    anyhow::bail!("port {} is still in use after stopping its process", port)
}

#[cfg(not(unix))]
pub fn kill_owners(_owners: &[PortOwner], port: u16) -> anyhow::Result<()> {
    anyhow::bail!(
        "stopping the process on port {} is not supported on this platform",
        port
    )
}

#[cfg(unix)]
fn wait_until_free(port: u16, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if is_available(port) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    is_available(port)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lsof_field_output() {
        let owners = parse_lsof("p26757\ncrojo\nf6\np11323\ncnode\nf13\nf14\n");
        assert_eq!(
            owners,
            vec![
                PortOwner {
                    pid: 26757,
                    name: "rojo".into()
                },
                PortOwner {
                    pid: 11323,
                    name: "node".into()
                },
            ]
        );
    }

    #[test]
    fn parses_empty_lsof_output() {
        assert!(parse_lsof("").is_empty());
    }

    #[test]
    fn loopback_listener_makes_port_unavailable() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        assert!(!is_available(listener.local_addr().unwrap().port()));
    }

    #[test]
    fn finds_next_port_after_a_busy_one() {
        let listener = std::net::TcpListener::bind("0.0.0.0:0").unwrap();
        let busy = listener.local_addr().unwrap().port();
        assert!(!is_available(busy));
        let next = next_available(busy).unwrap();
        assert!(next > busy);
    }
}
