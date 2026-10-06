//! Round-trip latency using unprivileged ICMP "ping sockets".
//!
//! These need no root as long as net.ipv4.ping_group_range includes the user's
//! group, which is the default on Fedora. If it doesn't, results stay `None`.

use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use socket2::{Domain, Protocol, Socket, Type};

use super::temps;
use crate::snapshot::PingTarget;

const INTERVAL: Duration = Duration::from_secs(1);
const TIMEOUT: Duration = Duration::from_millis(900);
/// A well-known public host, used as the "is the internet reachable" target.
const INTERNET_HOST: Ipv4Addr = Ipv4Addr::new(1, 1, 1, 1);

const ECHO_REQUEST: u8 = 8;
const ECHO_REPLY: u8 = 0;

type Latest = Arc<Mutex<(Option<Ipv4Addr>, Option<f32>)>>;

pub struct Pinger {
    targets: Vec<(&'static str, Latest)>,
}

impl Pinger {
    /// Starts one background thread per target so a timeout on one never delays the other.
    pub fn start() -> Self {
        let targets = vec![
            ("Gateway", spawn(default_gateway)),
            ("Internet", spawn(|| Some(INTERNET_HOST))),
        ];
        Self { targets }
    }

    pub fn latest(&self) -> Vec<PingTarget> {
        self.targets
            .iter()
            .filter_map(|(label, latest)| {
                let (host, ms) = *latest.lock().unwrap();
                Some(PingTarget { label: label.to_string(), host: host?.to_string(), ms })
            })
            .collect()
    }
}

/// `resolve` is called before every ping so the gateway follows network changes.
fn spawn(resolve: fn() -> Option<Ipv4Addr>) -> Latest {
    let latest = Latest::default();
    let shared = latest.clone();
    let _ = thread::Builder::new().name("ping".into()).spawn(move || {
        let socket = open_socket();
        let mut seq: u16 = 0;
        loop {
            let started = Instant::now();
            let host = resolve();
            seq = seq.wrapping_add(1);
            let ms = match (&socket, host) {
                (Some(s), Some(h)) => ping(s, h, seq),
                _ => None,
            };
            *shared.lock().unwrap() = (host, ms);
            thread::sleep(INTERVAL.saturating_sub(started.elapsed()));
        }
    });
    latest
}

fn open_socket() -> Option<UdpSocket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::ICMPV4)).ok()?;
    // Only used for sendto/recvfrom, which behave the same on any datagram socket.
    let socket: UdpSocket = socket.into();
    socket.set_read_timeout(Some(TIMEOUT)).ok()?;
    Some(socket)
}

/// Milliseconds until the echo reply, or None on timeout or error.
fn ping(socket: &UdpSocket, host: Ipv4Addr, seq: u16) -> Option<f32> {
    // Type, code, checksum, id, sequence. The kernel fills in checksum and id.
    let mut packet = [0u8; 16];
    packet[0] = ECHO_REQUEST;
    packet[6..8].copy_from_slice(&seq.to_be_bytes());

    let started = Instant::now();
    socket.send_to(&packet, SocketAddrV4::new(host, 0)).ok()?;
    let mut reply = [0u8; 64];
    // Late replies to earlier pings can arrive first; skip anything that isn't ours.
    while started.elapsed() < TIMEOUT {
        let (len, _) = socket.recv_from(&mut reply).ok()?;
        if len >= 8 && reply[0] == ECHO_REPLY && reply[6..8] == seq.to_be_bytes() {
            return Some(started.elapsed().as_secs_f32() * 1000.0);
        }
    }
    None
}

/// The default route's gateway from /proc/net/route.
fn default_gateway() -> Option<Ipv4Addr> {
    const RTF_GATEWAY: u32 = 0x2;
    let table = temps::read_string("/proc/net/route")?;
    table.lines().skip(1).find_map(|line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let flags = u32::from_str_radix(fields.get(3)?, 16).ok()?;
        if *fields.get(1)? != "00000000" || flags & RTF_GATEWAY == 0 {
            return None;
        }
        // The address is printed as a native-endian hex word.
        let raw = u32::from_str_radix(fields.get(2)?, 16).ok()?;
        Some(Ipv4Addr::from(raw.to_ne_bytes()))
    })
}
