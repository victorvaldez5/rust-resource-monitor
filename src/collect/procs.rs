use std::collections::HashMap;

use sysinfo::{Process, System, Users};

use super::owner::Owners;
use crate::snapshot::{ProcInfo, ProcUsage};

/// Executable file name when readable, otherwise the (15-char) kernel comm name.
/// Some programs install as `.../versions/2.1.291`; a name with no letters in it
/// says nothing, so those fall back to the comm name too.
pub fn display_name(p: &Process) -> String {
    p.exe()
        .and_then(|e| e.file_name())
        .map(|n| n.to_string_lossy().trim_end_matches(" (deleted)").to_string())
        .filter(|n| n.chars().any(char::is_alphabetic))
        .unwrap_or_else(|| p.name().to_string_lossy().into_owned())
}

/// Every process, ordered by pid, for the process tree.
///
/// `gpu` and `net` are per-pid figures from their collectors; `secs` is the
/// time since the previous sample, for turning disk byte counts into rates.
pub fn list(
    sys: &System,
    owners: &Owners,
    users: &Users,
    gpu: &HashMap<u32, [f64; 2]>,
    net: &HashMap<u32, [f64; 2]>,
    secs: f64,
) -> Vec<ProcInfo> {
    /// Long enough to recognise a command, short enough for a tooltip.
    const CMD_MAX: usize = 400;
    let cores = sys.cpus().len().max(1) as f32;
    let mut out: Vec<ProcInfo> = sys
        .processes()
        .iter()
        .map(|(pid, p)| {
            let owner = owners.get(*pid);
            let user = p.user_id().map(|uid| match users.get_user_by_id(uid) {
                Some(user) => user.name().to_string(),
                None => uid.to_string(),
            });
            let cmd: Vec<_> = p.cmd().iter().map(|arg| arg.to_string_lossy()).collect();
            let io = p.disk_usage();
            let gpu = gpu.get(&pid.as_u32()).copied().unwrap_or_default();
            let net = net.get(&pid.as_u32()).copied().unwrap_or_default();
            ProcInfo {
                pid: pid.as_u32(),
                parent: p.parent().map(|parent| parent.as_u32()),
                name: display_name(p),
                user: user.unwrap_or_default(),
                group: owner.group,
                unit: owner.unit,
                cmd: cmd.join(" ").chars().take(CMD_MAX).collect(),
                usage: ProcUsage {
                    cpu: p.cpu_usage() / cores,
                    mem: p.memory(),
                    gpu: gpu[1] as f32,
                    vram: gpu[0] as u64,
                    disk_read: io.read_bytes as f64 / secs,
                    disk_write: io.written_bytes as f64 / secs,
                    net_down: net[0],
                    net_up: net[1],
                },
                start_time: p.start_time(),
            }
        })
        .collect();
    out.sort_by_key(|p| p.pid);
    out
}
