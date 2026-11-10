use std::process::Command;
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

impl std::fmt::Display for PortOwner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.name.is_empty() {
            write!(f, "PID {}", self.pid)
        } else {
            write!(f, "{} (PID {})", self.name, self.pid)
        }
    }
}

fn command_stdout(cmd: &str, args: &[&str]) -> String {
    Command::new(cmd)
        .args(args)
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
        .unwrap_or_default()
}

#[cfg(not(windows))]
pub fn owners(port: u16) -> Vec<PortOwner> {
    let lsof = parse_lsof(&command_stdout(
        "lsof",
        &["-nP", &format!("-iTCP:{port}"), "-sTCP:LISTEN", "-Fpc"],
    ));
    if !lsof.is_empty() {
        return lsof;
    }
    parse_ss(&command_stdout(
        "ss",
        &["-ltnp", "sport", "=", &format!(":{port}")],
    ))
}

#[cfg(windows)]
pub fn owners(port: u16) -> Vec<PortOwner> {
    parse_netstat(&command_stdout("netstat", &["-ano"]), port)
        .into_iter()
        .map(|pid| PortOwner {
            pid,
            name: parse_tasklist_name(&command_stdout(
                "tasklist",
                &["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"],
            ))
            .unwrap_or_default(),
        })
        .collect()
}

#[cfg(any(not(windows), test))]
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

#[cfg(any(not(windows), test))]
fn parse_ss(output: &str) -> Vec<PortOwner> {
    let user = regex::Regex::new(r#"\("([^"]*)",pid=(\d+)"#).expect("valid ss users regex");
    let mut owners: Vec<PortOwner> = Vec::new();
    for capture in user.captures_iter(output) {
        let Ok(pid) = capture[2].parse() else {
            continue;
        };
        if !owners.iter().any(|o| o.pid == pid) {
            owners.push(PortOwner {
                pid,
                name: capture[1].to_string(),
            });
        }
    }
    owners
}

#[cfg(any(windows, test))]
fn parse_netstat(output: &str, port: u16) -> Vec<u32> {
    let suffix = format!(":{port}");
    let mut pids: Vec<u32> = Vec::new();
    for line in output.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let [proto, local, remote, .., pid] = fields[..] else {
            continue;
        };
        let listening = remote == "0.0.0.0:0" || remote == "[::]:0";
        if !proto.eq_ignore_ascii_case("TCP") || !local.ends_with(&suffix) || !listening {
            continue;
        }
        if let Ok(pid) = pid.parse() {
            if pid != 0 && !pids.contains(&pid) {
                pids.push(pid);
            }
        }
    }
    pids
}

#[cfg(any(windows, test))]
fn parse_tasklist_name(output: &str) -> Option<String> {
    let name = output.trim().strip_prefix('"')?.split('"').next()?;
    (!name.is_empty()).then(|| name.to_string())
}

#[cfg(unix)]
fn terminate(owner: &PortOwner, force: bool) -> anyhow::Result<()> {
    use nix::sys::signal::{kill, Signal};

    let signal = if force {
        Signal::SIGKILL
    } else {
        Signal::SIGTERM
    };
    kill(nix::unistd::Pid::from_raw(owner.pid as i32), signal)
        .map_err(|e| anyhow::anyhow!("could not stop {}: {}", owner, e))
}

#[cfg(windows)]
fn terminate(owner: &PortOwner, _force: bool) -> anyhow::Result<()> {
    crate::services::process_manager::taskkill_tree(owner.pid)
        .map_err(|e| anyhow::anyhow!("could not stop {}: {}", owner, e))
}

pub fn kill_owners(owners: &[PortOwner], port: u16) -> anyhow::Result<()> {
    for owner in owners {
        terminate(owner, false)?;
    }
    if wait_until_free(port, Duration::from_secs(3)) {
        return Ok(());
    }
    for owner in owners {
        let _ = terminate(owner, true);
    }
    if wait_until_free(port, Duration::from_secs(2)) {
        return Ok(());
    }
    anyhow::bail!("port {} is still in use after stopping its process", port)
}

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
    fn parses_ss_users_and_dedupes_pids() {
        let output = concat!(
            "State  Recv-Q Send-Q Local Address:Port  Peer Address:Port Process\n",
            "LISTEN 0      128        127.0.0.1:34872      0.0.0.0:*     users:((\"rojo\",pid=4242,fd=9))\n",
            "LISTEN 0      511                *:3000             *:*     users:((\"node\",pid=77,fd=21),(\"node\",pid=78,fd=21))\n",
            "LISTEN 0      128            [::1]:34872         [::]:*     users:((\"rojo\",pid=4242,fd=10))\n",
        );
        let owners = parse_ss(output);
        let summary: Vec<(u32, &str)> = owners.iter().map(|o| (o.pid, o.name.as_str())).collect();
        assert_eq!(summary, vec![(4242, "rojo"), (77, "node"), (78, "node")]);
    }

    #[test]
    fn parses_ss_output_without_process_info() {
        assert!(parse_ss("LISTEN 0 128 127.0.0.1:34872 0.0.0.0:*\n").is_empty());
    }

    #[test]
    fn parses_netstat_listeners_for_the_port_only() {
        let output = concat!(
            "\r\nActive Connections\r\n\r\n",
            "  Proto  Local Address          Foreign Address        State           PID\r\n",
            "  TCP    0.0.0.0:135            0.0.0.0:0              LISTENING       1120\r\n",
            "  TCP    127.0.0.1:34872        0.0.0.0:0              LISTENING       9120\r\n",
            "  TCP    127.0.0.1:348720       0.0.0.0:0              LISTENING       5\r\n",
            "  TCP    127.0.0.1:51000        127.0.0.1:34872        ESTABLISHED     7000\r\n",
            "  TCP    127.0.0.1:34872        127.0.0.1:51000        ESTABLISHED     9120\r\n",
            "  TCP    [::]:34872             [::]:0                 ABH\u{d6}REN         9121\r\n",
            "  TCP    [::1]:34872            [::]:0                 LISTENING       9120\r\n",
        );
        assert_eq!(parse_netstat(output, 34872), vec![9120, 9121]);
    }

    #[test]
    fn parses_tasklist_csv_name() {
        assert_eq!(
            parse_tasklist_name("\"rojo.exe\",\"9120\",\"Console\",\"1\",\"18,244 K\"\r\n"),
            Some("rojo.exe".to_string())
        );
        assert_eq!(
            parse_tasklist_name(
                "INFO: No tasks are running which match the specified criteria.\r\n"
            ),
            None
        );
    }

    #[test]
    fn owner_display_omits_missing_name() {
        let named = PortOwner {
            pid: 7,
            name: "rojo".into(),
        };
        let unnamed = PortOwner {
            pid: 7,
            name: String::new(),
        };
        assert_eq!(named.to_string(), "rojo (PID 7)");
        assert_eq!(unnamed.to_string(), "PID 7");
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
