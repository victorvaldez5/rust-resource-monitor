use std::collections::HashMap;

use sysinfo::{Pid, Process, System};

use crate::snapshot::TopProc;

/// How many rows each "top consumers" table shows.
pub const TOP_N: usize = 5;

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

pub fn name_of_pid(sys: &System, pid: u32) -> String {
    sys.process(Pid::from_u32(pid))
        .map(display_name)
        .unwrap_or_else(|| format!("pid {pid}"))
}

/// Sums rows that share a name and returns the `TOP_N` largest by `key`,
/// dropping anything that isn't using the resource at all.
pub fn top(
    rows: impl Iterator<Item = (String, [f64; 2])>,
    key: impl Fn(&[f64; 2]) -> f64,
) -> Vec<TopProc> {
    let mut groups: HashMap<String, TopProc> = HashMap::new();
    for (name, values) in rows {
        let g = groups.entry(name).or_default();
        g.count += 1;
        g.values[0] += values[0];
        g.values[1] += values[1];
    }
    let mut out: Vec<TopProc> = groups
        .into_iter()
        .filter(|(_, g)| key(&g.values) > 0.0)
        .map(|(name, g)| TopProc { name, ..g })
        .collect();
    out.sort_by(|a, b| key(&b.values).total_cmp(&key(&a.values)).then(a.name.cmp(&b.name)));
    out.truncate(TOP_N);
    out
}
