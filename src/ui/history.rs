//! The process history: every process seen since the monitor started, running or
//! not, with a column per resource. Processes sharing a name are folded into one
//! row with their usage added up; click it to see each process id.

use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

use eframe::egui::{self, Align, Layout, RichText, Ui};

use super::panel::{fmt_bytes, fmt_rate};
use super::tree::fmt_duration;
use crate::snapshot::{ProcRecord, ProcUsage};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Col {
    Name,
    Pid,
    User,
    Group,
    Uptime,
    Active,
    Cpu,
    Ram,
    Gpu,
    Vram,
    DiskRead,
    DiskWrite,
    NetDown,
    NetUp,
}

/// Every column to the right of the name, left to right, with its width.
const COLUMNS: [(Col, &str, f32); 13] = [
    (Col::Pid, "PID", 56.0),
    (Col::User, "User", 88.0),
    (Col::Group, "Belongs to", 110.0),
    (Col::Uptime, "Uptime", 72.0),
    (Col::Active, "Active", 80.0),
    (Col::Cpu, "CPU", 54.0),
    (Col::Ram, "RAM", 72.0),
    (Col::Gpu, "GPU", 50.0),
    (Col::Vram, "VRAM", 72.0),
    (Col::DiskRead, "Disk read", 84.0),
    (Col::DiskWrite, "Disk write", 84.0),
    (Col::NetDown, "Net down", 84.0),
    (Col::NetUp, "Net up", 84.0),
];

#[derive(Clone, Copy)]
pub struct Sort {
    key: Col,
    descending: bool,
}

impl Default for Sort {
    /// Most recently active first.
    fn default() -> Self {
        Self {
            key: Col::Active,
            descending: true,
        }
    }
}

impl Sort {
    /// Clicking the sorted column flips it; clicking another sorts by it, with
    /// names and pids running low to high and the rest biggest or newest first.
    fn click(&mut self, key: Col) {
        if self.key == key {
            self.descending = !self.descending;
        } else {
            *self = Self {
                key,
                descending: !matches!(key, Col::Name | Col::Pid | Col::User | Col::Group),
            };
        }
    }

    fn arrow(&self, key: Col) -> &'static str {
        match (self.key == key, self.descending) {
            (false, _) => "",
            (true, true) => " ⏷",
            (true, false) => " ⏶",
        }
    }
}

fn sort(records: &mut [ProcRecord], sort: Sort) {
    let f = |r: &ProcRecord| -> f64 {
        let u = &r.usage;
        match sort.key {
            Col::Name | Col::User | Col::Group => 0.0,
            Col::Pid => r.pid as f64,
            // Newer start means shorter uptime, so the oldest sorts as biggest.
            Col::Uptime => -(r.start_time as f64),
            Col::Active => 0.0,
            Col::Cpu => u.cpu as f64,
            Col::Ram => u.mem as f64,
            Col::Gpu => u.gpu as f64,
            Col::Vram => u.vram as f64,
            Col::DiskRead => u.disk_read,
            Col::DiskWrite => u.disk_write,
            Col::NetDown => u.net_down,
            Col::NetUp => u.net_up,
        }
    };
    records.sort_by(|a, b| {
        let ord = match sort.key {
            Col::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            Col::User => a.user.to_lowercase().cmp(&b.user.to_lowercase()),
            Col::Group => a.group.to_lowercase().cmp(&b.group.to_lowercase()),
            // Running processes count as active right now, so they rank above any that exited.
            Col::Active => (a.running, a.last_active).cmp(&(b.running, b.last_active)),
            _ => f(a).total_cmp(&f(b)),
        };
        let ord = if sort.descending { ord.reverse() } else { ord };
        // Keep ties in a stable, readable order.
        ord.then_with(|| a.name.cmp(&b.name))
            .then(a.pid.cmp(&b.pid))
    });
}

/// Text columns line up on the left, numbers on the right.
fn cell_layout(col: Col) -> Layout {
    match col {
        Col::User | Col::Group => Layout::left_to_right(Align::Center),
        _ => Layout::right_to_left(Align::Center),
    }
}

fn value(r: &ProcRecord, count: usize, col: Col, now: u64) -> String {
    let u = &r.usage;
    let nonzero = |on: bool, text: String| if on { text } else { "–".to_string() };
    match col {
        Col::Name => r.name.clone(),
        Col::Pid if count > 1 => format!("×{count}"),
        Col::Pid => r.pid.to_string(),
        Col::User => r.user.clone(),
        Col::Group => r.group.clone(),
        Col::Uptime => {
            fmt_duration(if r.running { now } else { r.last_seen }.saturating_sub(r.start_time))
        }
        Col::Active => match r.last_active {
            Some(t) if now.saturating_sub(t) < 2 => "now".to_string(),
            Some(t) => format!("{} ago", fmt_duration(now.saturating_sub(t))),
            None => "never".to_string(),
        },
        Col::Cpu => nonzero(u.cpu > 0.0, format!("{:.1}%", u.cpu)),
        Col::Ram => nonzero(u.mem > 0, fmt_bytes(u.mem as f64)),
        Col::Gpu => nonzero(u.gpu > 0.0, format!("{:.0}%", u.gpu)),
        Col::Vram => nonzero(u.vram > 0, fmt_bytes(u.vram as f64)),
        Col::DiskRead => nonzero(u.disk_read > 0.0, fmt_rate(u.disk_read)),
        Col::DiskWrite => nonzero(u.disk_write > 0.0, fmt_rate(u.disk_write)),
        Col::NetDown => nonzero(u.net_down > 0.0, fmt_rate(u.net_down)),
        Col::NetUp => nonzero(u.net_up > 0.0, fmt_rate(u.net_up)),
    }
}

/// One drawn line: a name's total, or one process under it.
struct Line {
    record: ProcRecord,
    count: usize,
    child: bool,
    open: bool,
}

/// Adds up every process that shares a name.
fn combine(members: &[&ProcRecord]) -> ProcRecord {
    let first = members[0];
    let mut total = ProcRecord {
        pid: first.pid,
        name: first.name.clone(),
        group: first.group.clone(),
        user: first.user.clone(),
        start_time: u64::MAX,
        samples: 1,
        ..Default::default()
    };
    let mut usage = ProcUsage::default();
    for r in members {
        let u = &r.usage;
        usage.cpu += u.cpu;
        usage.mem += u.mem;
        usage.gpu += u.gpu;
        usage.vram += u.vram;
        usage.disk_read += u.disk_read;
        usage.disk_write += u.disk_write;
        usage.net_down += u.net_down;
        usage.net_up += u.net_up;
        total.start_time = total.start_time.min(r.start_time);
        total.last_seen = total.last_seen.max(r.last_seen);
        total.last_active = total.last_active.max(r.last_active);
        total.cpu_sum += r.avg_cpu();
        total.running |= r.running;
        if r.user != total.user {
            total.user = "various".to_string();
        }
        if r.group != total.group {
            total.group = "various".to_string();
        }
    }
    total.usage = usage;
    total
}

pub fn show(ui: &mut Ui, records: &[ProcRecord], state: &mut Sort, expanded: &mut HashSet<String>) {
    let running = records.iter().filter(|r| r.running).count();
    ui.weak(format!(
        "{running} running · {} exited since the monitor started",
        records.len() - running
    ));

    let height = ui.spacing().interact_size.y;
    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            for (col, title, width) in COLUMNS.iter().rev() {
                ui.allocate_ui_with_layout(egui::vec2(*width, height), cell_layout(*col), |ui| {
                    ui.set_min_width(*width);
                    let label = RichText::new(format!("{title}{}", state.arrow(*col))).strong();
                    if ui.add(egui::Button::new(label).frame(false)).clicked() {
                        state.click(*col);
                    }
                });
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                let label = RichText::new(format!("Process{}", state.arrow(Col::Name))).strong();
                if ui.add(egui::Button::new(label).frame(false)).clicked() {
                    state.click(Col::Name);
                }
            });
        });
    });
    ui.separator();

    let mut by_name: HashMap<&str, Vec<&ProcRecord>> = HashMap::new();
    for r in records {
        by_name.entry(&r.name).or_default().push(r);
    }
    let mut groups: Vec<(ProcRecord, Vec<ProcRecord>)> = by_name
        .into_values()
        .map(|members| (combine(&members), members.into_iter().cloned().collect()))
        .collect();
    let mut totals: Vec<ProcRecord> = groups.iter().map(|g| g.0.clone()).collect();
    sort(&mut totals, *state);
    groups.sort_by_key(|g| totals.iter().position(|t| t.name == g.0.name));

    // The lines to draw: a row per name, then its processes if it is open.
    let mut lines: Vec<Line> = Vec::new();
    for (total, mut members) in groups {
        let count = members.len();
        let open = count > 1 && expanded.contains(&total.name);
        lines.push(Line {
            record: total,
            count,
            child: false,
            open,
        });
        if open {
            sort(&mut members, *state);
            lines.extend(members.into_iter().map(|record| Line {
                record,
                count: 1,
                child: true,
                open: false,
            }));
        }
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    egui::ScrollArea::vertical().auto_shrink(false).show_rows(
        ui,
        height,
        lines.len(),
        |ui, range| {
            for line in &lines[range] {
                let (r, count) = (&line.record, line.count);
                ui.horizontal(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        for (col, _, width) in COLUMNS.iter().rev() {
                            ui.allocate_ui_with_layout(
                                egui::vec2(*width, height),
                                cell_layout(*col),
                                |ui| {
                                    ui.set_min_width(*width);
                                    let text = RichText::new(value(r, count, *col, now));
                                    let text = if matches!(col, Col::User | Col::Group) {
                                        text
                                    } else {
                                        text.monospace()
                                    };
                                    ui.add(
                                        egui::Label::new(if r.running {
                                            text
                                        } else {
                                            text.weak()
                                        })
                                        .truncate(),
                                    );
                                },
                            );
                        }
                        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                            let arrow = match (count > 1, line.open) {
                                (false, _) => "",
                                (true, true) => "⏷ ",
                                (true, false) => "⏵ ",
                            };
                            let indent = if line.child { "      " } else { "" };
                            let name = RichText::new(format!("{indent}{arrow}{}", r.name));
                            let name = if r.running { name } else { name.weak() };
                            let label = egui::Label::new(name)
                                .truncate()
                                .sense(egui::Sense::click());
                            let response = ui.add(label);
                            if response.clicked() && count > 1 && !expanded.remove(&r.name) {
                                expanded.insert(r.name.clone());
                            }
                            response.on_hover_ui(|ui| {
                                if count > 1 {
                                    ui.strong(format!("{} ({count} processes)", r.name));
                                } else {
                                    ui.strong(format!("{} (pid {})", r.name, r.pid));
                                }
                                ui.label(if r.running { "Running" } else { "Exited" });
                                if !r.group.is_empty() {
                                    ui.weak(format!("Belongs to {}", r.group));
                                }
                                ui.weak(format!("Average CPU {:.1}%", r.avg_cpu()));
                            });
                        });
                    });
                });
            }
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(name: &str, pid: u32, cpu: f32, user: &str) -> ProcRecord {
        ProcRecord {
            pid,
            name: name.into(),
            user: user.into(),
            running: true,
            samples: 1,
            usage: ProcUsage {
                cpu,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn combine_adds_usage_and_flags_mixed_users() {
        let (a, b) = (record("zen", 1, 1.5, "me"), record("zen", 2, 2.0, "root"));
        let total = combine(&[&a, &b]);
        assert_eq!(total.usage.cpu, 3.5);
        assert_eq!(total.user, "various");
    }

    #[test]
    fn first_click_sorts_names_a_to_z_and_usage_biggest_first() {
        let mut sort_state = Sort::default();
        sort_state.click(Col::Name);
        assert!(!sort_state.descending);
        sort_state.click(Col::Cpu);
        assert!(sort_state.descending);
        sort_state.click(Col::Cpu);
        assert!(!sort_state.descending);
    }

    #[test]
    fn sorts_by_cpu_descending() {
        let mut rows = vec![record("a", 1, 1.0, ""), record("b", 2, 9.0, "")];
        sort(
            &mut rows,
            Sort {
                key: Col::Cpu,
                descending: true,
            },
        );
        assert_eq!(rows[0].name, "b");
    }
}
