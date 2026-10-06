use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::snapshot::{ProcInfo, ProcRecord};

/// Exited processes kept; the oldest are forgotten beyond this.
const MAX_EXITED: usize = 1000;
/// Start times of one process can differ by a second or so between samples,
/// because sysinfo works them out from a boot time that drifts.
const START_TOLERANCE: u64 = 5;
/// Percent of all cores a process must use in a sample to count as active.
const ACTIVE_CPU: f32 = 0.1;

/// Remembers every process seen since the monitor started, including those that
/// have since exited.
#[derive(Default)]
pub struct ProcLog {
    records: HashMap<u64, ProcRecord>,
    /// Record id of each pid currently running.
    live: HashMap<u32, u64>,
    next_id: u64,
}

impl ProcLog {
    pub fn update(&mut self, procs: &[ProcInfo]) -> Arc<Vec<ProcRecord>> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        for r in self.records.values_mut() {
            r.running = false;
            r.usage = Default::default();
        }
        // A pid whose start time has moved on was reused by a new process.
        let mut live = HashMap::new();
        for p in procs {
            let existing =
                self.live.get(&p.pid).copied().filter(|id| {
                    self.records[id].start_time.abs_diff(p.start_time) <= START_TOLERANCE
                });
            let id = existing.unwrap_or_else(|| {
                self.next_id += 1;
                self.next_id
            });
            live.insert(p.pid, id);
            let r = self.records.entry(id).or_insert_with(|| ProcRecord {
                pid: p.pid,
                name: p.name.clone(),
                user: p.user.clone(),
                group: p.group.clone(),
                start_time: p.start_time,
                last_seen: now,
                last_active: None,
                samples: 0,
                cpu_sum: 0.0,
                usage: p.usage,
                running: true,
            });
            r.running = true;
            r.usage = p.usage;
            r.last_seen = now;
            r.samples += 1;
            r.cpu_sum += p.usage.cpu as f64;
            if p.usage.cpu >= ACTIVE_CPU {
                r.last_active = Some(now);
            }
        }

        self.live = live;

        let mut exited: Vec<_> = self
            .records
            .iter()
            .filter(|(_, r)| !r.running)
            .map(|(k, r)| (*k, r.last_seen))
            .collect();
        if exited.len() > MAX_EXITED {
            exited.sort_by_key(|(_, seen)| *seen);
            for (key, _) in &exited[..exited.len() - MAX_EXITED] {
                self.records.remove(key);
            }
        }
        Arc::new(self.records.values().cloned().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::ProcUsage;

    fn proc(pid: u32, start_time: u64, cpu: f32) -> ProcInfo {
        ProcInfo {
            pid,
            name: format!("p{pid}"),
            start_time,
            usage: ProcUsage {
                cpu,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn drifting_start_time_is_the_same_process() {
        let mut log = ProcLog::default();
        log.update(&[proc(10, 1000, 0.0)]);
        let records = log.update(&[proc(10, 1001, 0.0)]);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].samples, 2);
    }

    #[test]
    fn reused_pid_is_a_new_process() {
        let mut log = ProcLog::default();
        log.update(&[proc(10, 1000, 0.0)]);
        let records = log.update(&[proc(10, 5000, 0.0)]);
        assert_eq!(records.len(), 2);
        assert_eq!(records.iter().filter(|r| r.running).count(), 1);
    }

    #[test]
    fn exited_process_is_kept_and_idle() {
        let mut log = ProcLog::default();
        log.update(&[proc(10, 1000, 50.0)]);
        let records = log.update(&[]);
        assert_eq!(records.len(), 1);
        assert!(!records[0].running);
        assert_eq!(records[0].usage.cpu, 0.0);
        assert!(records[0].last_active.is_some());
    }

    #[test]
    fn only_the_newest_exited_are_kept() {
        let mut log = ProcLog::default();
        let all: Vec<_> = (0..MAX_EXITED as u32 + 10)
            .map(|i| proc(i, 1000, 0.0))
            .collect();
        log.update(&all);
        assert_eq!(log.update(&[]).len(), MAX_EXITED);
    }
}
