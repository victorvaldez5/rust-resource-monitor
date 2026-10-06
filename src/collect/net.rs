use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::process::Command;

use sysinfo::System;

use super::ping::Pinger;
use super::temps;
use crate::snapshot::{NetIface, NetInfo};

/// Listing sockets costs far more than everything else we sample, so the
/// per-app table refreshes on every Nth sample instead of every one.
const SOCKET_SAMPLE_EVERY: u32 = 2;
/// Byte counters of one TCP connection at the previous socket sample.
struct Conn {
    sent: u64,
    received: u64,
}

pub struct Net {
    /// (rx, tx) byte counters per interface at the previous sample.
    last_bytes: HashMap<String, (u64, u64)>,
    conns: HashMap<String, Conn>,
    /// Socket inode -> owning pid; None when it belongs to another user.
    owners: HashMap<u64, Option<u32>>,
    /// False until the first socket sample, whose totals are history, not traffic.
    primed: bool,
    tick: u32,
    since_socket_sample: f64,
    /// pid -> (download, upload) bytes/s as of the last socket sample.
    per_pid: HashMap<u32, [f64; 2]>,
    pinger: Pinger,
}

impl Net {
    pub fn new() -> Self {
        let mut this = Self {
            last_bytes: HashMap::new(),
            conns: HashMap::new(),
            owners: HashMap::new(),
            primed: false,
            tick: 0,
            since_socket_sample: 0.0,
            per_pid: HashMap::new(),
            pinger: Pinger::start(),
        };
        this.sample_ifaces(1.0);
        this
    }

    /// `secs` is the time since the previous sample.
    pub fn sample(&mut self, sys: &System, secs: f64) -> NetInfo {
        let ifaces = self.sample_ifaces(secs);
        self.since_socket_sample += secs;

        if self.tick.is_multiple_of(SOCKET_SAMPLE_EVERY) {
            self.sample_sockets(sys);
            self.since_socket_sample = 0.0;
        }
        self.tick = self.tick.wrapping_add(1);

        NetInfo {
            ifaces: ifaces.into_iter().filter(|i| i.up).collect(),
            pings: self.pinger.latest(),
        }
    }

    /// pid -> (download, upload) bytes/s over TCP, for processes we can see.
    pub fn per_pid(&self) -> &HashMap<u32, [f64; 2]> {
        &self.per_pid
    }

    /// Physical interfaces only: tunnels like VPNs would count the same bytes twice.
    fn sample_ifaces(&mut self, secs: f64) -> Vec<NetIface> {
        let mut dirs = temps::subdirs("/sys/class/net");
        dirs.sort();
        let mut out = Vec::new();
        for dir in dirs.iter().filter(|d| d.join("device").exists()) {
            let name = dir
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let counter = |file: &str| temps::read_u64(dir.join("statistics").join(file));
            let (Some(rx), Some(tx)) = (counter("rx_bytes"), counter("tx_bytes")) else {
                continue;
            };
            let (prev_rx, prev_tx) = self
                .last_bytes
                .insert(name.clone(), (rx, tx))
                .unwrap_or((rx, tx));
            out.push(NetIface {
                up: temps::read_string(dir.join("operstate")).as_deref() == Some("up"),
                temp: iface_temp(dir),
                down_rate: rx.saturating_sub(prev_rx) as f64 / secs,
                up_rate: tx.saturating_sub(prev_tx) as f64 / secs,
                name,
            });
        }
        out
    }

    /// Rebuilds `self.per_pid` from the change in every TCP connection's byte counters.
    fn sample_sockets(&mut self, sys: &System) {
        // -t TCP, -i byte counters, -n numeric, -e socket inode, -H no header, -O one line each.
        let Ok(output) = Command::new("ss").arg("-tineHO").output() else {
            return;
        };
        let text = String::from_utf8_lossy(&output.stdout);

        let mut conns = HashMap::new();
        // (inode, received bytes, sent bytes) for connections that moved data.
        let mut active: Vec<(u64, u64, u64)> = Vec::new();
        for line in text.lines() {
            let Some((key, inode, now)) = parse_socket(line) else {
                continue;
            };
            let (sent, received) = match self.conns.get(&key) {
                Some(prev) if now.sent >= prev.sent && now.received >= prev.received => {
                    (now.sent - prev.sent, now.received - prev.received)
                }
                // First sight of a connection opened since the last sample.
                _ if self.primed => (now.sent, now.received),
                _ => (0, 0),
            };
            if sent + received > 0 {
                active.push((inode, received, sent));
            }
            conns.insert(key, now);
        }
        self.conns = conns;
        self.primed = true;

        self.resolve_owners(sys, &active);
        let secs = self.since_socket_sample.max(0.001);
        let mut per_pid: HashMap<u32, [f64; 2]> = HashMap::new();
        for (inode, received, sent) in &active {
            let Some(Some(pid)) = self.owners.get(inode) else {
                continue;
            };
            let rates = per_pid.entry(*pid).or_default();
            rates[0] += *received as f64 / secs;
            rates[1] += *sent as f64 / secs;
        }
        self.per_pid = per_pid;
    }

    /// Fills `self.owners` for any active socket we haven't matched to a process yet.
    fn resolve_owners(&mut self, sys: &System, active: &[(u64, u64, u64)]) {
        let live: HashSet<u64> = active.iter().map(|a| a.0).collect();
        self.owners.retain(|inode, _| live.contains(inode));
        let wanted: HashSet<u64> = live
            .iter()
            .copied()
            .filter(|i| !self.owners.contains_key(i))
            .collect();
        if wanted.is_empty() {
            return;
        }
        // Walk every readable /proc/<pid>/fd looking for "socket:[inode]" links.
        // Other users' processes aren't readable, so their sockets stay unmatched.
        for pid in sys.processes().keys() {
            let Ok(fds) = fs::read_dir(format!("/proc/{pid}/fd")) else {
                continue;
            };
            for fd in fds.flatten() {
                let Ok(target) = fs::read_link(fd.path()) else {
                    continue;
                };
                let inode = target.to_str().and_then(|t| {
                    t.strip_prefix("socket:[")?
                        .strip_suffix(']')?
                        .parse::<u64>()
                        .ok()
                });
                if let Some(inode) = inode.filter(|i| wanted.contains(i)) {
                    self.owners.entry(inode).or_insert(Some(pid.as_u32()));
                }
            }
        }
        for inode in wanted {
            self.owners.entry(inode).or_insert(None);
        }
    }
}

/// Parses one `ss -tineHO` line into (connection key, socket inode, counters).
/// Loopback connections are skipped: they never touch a network interface.
fn parse_socket(line: &str) -> Option<(String, u64, Conn)> {
    let mut fields = line.split_whitespace();
    let local = fields.nth(3)?;
    let peer = fields.next()?;
    if peer.starts_with("127.") || peer.starts_with("[::1]") {
        return None;
    }
    let (mut inode, mut sent, mut received) = (None, 0, 0);
    for field in fields {
        if let Some(v) = field.strip_prefix("ino:") {
            inode = v.parse::<u64>().ok();
        } else if let Some(v) = field.strip_prefix("bytes_acked:") {
            sent = v.parse().unwrap_or(0);
        } else if let Some(v) = field.strip_prefix("bytes_received:") {
            received = v.parse().unwrap_or(0);
        }
    }
    let inode = inode?;
    Some((
        format!("{local}>{peer}#{inode}"),
        inode,
        Conn { sent, received },
    ))
}

/// Wi-Fi cards hang their sensor off the radio, wired ones off the PHY.
fn iface_temp(iface: &Path) -> Option<f32> {
    ["phy80211", "phydev", "device"]
        .iter()
        .find_map(|sub| temps::hwmon_under(&iface.join(sub)))
        .and_then(|hwmon| temps::first_temp(&hwmon))
}
