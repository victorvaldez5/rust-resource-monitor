//! The process tree: every process under the one that started it.

use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

use eframe::egui::{self, Align, Layout, RichText, Sense, Ui};

use super::panel::fmt_bytes;
use crate::snapshot::ProcInfo;

const INDENT: f32 = 16.0;
/// Widths of the fixed columns to the right of the name.
const PID_WIDTH: f32 = 64.0;
const USER_WIDTH: f32 = 96.0;
const GROUP_WIDTH: f32 = 120.0;
const CPU_WIDTH: f32 = 60.0;
const MEM_WIDTH: f32 = 84.0;

/// One visible line of the tree.
pub struct Row {
    /// Index into the process list.
    pub index: usize,
    pub depth: usize,
    pub children: usize,
}

/// Flattens the tree into the lines to draw, skipping what's under a collapsed process.
/// `procs` must be ordered by pid; siblings come out in the same order.
pub fn rows(procs: &[ProcInfo], collapsed: &HashSet<u32>) -> Vec<Row> {
    let pids: HashSet<u32> = procs.iter().map(|p| p.pid).collect();
    let mut children: HashMap<u32, Vec<usize>> = HashMap::new();
    let mut roots = Vec::new();
    for (i, p) in procs.iter().enumerate() {
        match p
            .parent
            .filter(|parent| *parent != p.pid && pids.contains(parent))
        {
            Some(parent) => children.entry(parent).or_default().push(i),
            None => roots.push(i),
        }
    }

    let mut out = Vec::new();
    let mut stack: Vec<(usize, usize)> = roots.into_iter().rev().map(|i| (i, 0)).collect();
    while let Some((index, depth)) = stack.pop() {
        let kids = children
            .get(&procs[index].pid)
            .map_or(&[][..], Vec::as_slice);
        out.push(Row {
            index,
            depth,
            children: kids.len(),
        });
        if !collapsed.contains(&procs[index].pid) {
            stack.extend(kids.iter().rev().map(|kid| (*kid, depth + 1)));
        }
    }
    out
}

pub fn show(ui: &mut Ui, procs: &[ProcInfo], collapsed: &mut HashSet<u32>) {
    ui.horizontal(|ui| {
        ui.weak(format!("{} processes", procs.len()));
        if ui.small_button("Expand all").clicked() {
            collapsed.clear();
        }
        if ui.small_button("Collapse all").clicked() {
            collapsed.extend(procs.iter().map(|p| p.pid));
        }
    });
    let header = ["PID", "User", "Belongs to", "CPU", "Memory"].map(|h| RichText::new(h).weak());
    line(ui, header, |ui| {
        ui.weak("Process");
    });
    ui.separator();

    let rows = rows(procs, collapsed);
    let by_pid: HashMap<u32, &ProcInfo> = procs.iter().map(|p| (p.pid, p)).collect();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let height = ui.spacing().interact_size.y;
    // Only the lines scrolled into view are laid out.
    egui::ScrollArea::vertical().auto_shrink(false).show_rows(
        ui,
        height,
        rows.len(),
        |ui, range| {
            for row in &rows[range] {
                let p = &procs[row.index];
                let cells = [
                    RichText::new(p.pid.to_string()).monospace(),
                    RichText::new(&p.user),
                    RichText::new(&p.group),
                    RichText::new(format!("{:.1}%", p.usage.cpu)).monospace(),
                    RichText::new(fmt_bytes(p.usage.mem as f64)).monospace(),
                ];
                line(ui, cells, |ui| {
                    ui.add_space(row.depth as f32 * INDENT);
                    let arrow = match (row.children, collapsed.contains(&p.pid)) {
                        (0, _) => "    ",
                        (_, true) => "⏵ ",
                        (_, false) => "⏷ ",
                    };
                    let label = egui::Label::new(format!("{arrow}{}", p.name))
                        .truncate()
                        .sense(Sense::click());
                    let response = ui.add(label).on_hover_ui(|ui| details(ui, p, &by_pid, now));
                    if response.clicked() && row.children > 0 && !collapsed.remove(&p.pid) {
                        collapsed.insert(p.pid);
                    }
                });
            }
        },
    );
}

/// One line: fixed-width columns on the right, and `name` in whatever is left.
fn line(ui: &mut Ui, cells: [RichText; 5], name: impl FnOnce(&mut Ui)) {
    let [pid, user, group, cpu, mem] = cells;
    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            cell(ui, MEM_WIDTH, Layout::right_to_left(Align::Center), mem);
            cell(ui, CPU_WIDTH, Layout::right_to_left(Align::Center), cpu);
            cell(ui, GROUP_WIDTH, Layout::left_to_right(Align::Center), group);
            cell(ui, USER_WIDTH, Layout::left_to_right(Align::Center), user);
            cell(ui, PID_WIDTH, Layout::right_to_left(Align::Center), pid);
            ui.with_layout(Layout::left_to_right(Align::Center), name);
        });
    });
}

fn cell(ui: &mut Ui, width: f32, layout: Layout, text: RichText) {
    ui.allocate_ui_with_layout(
        egui::vec2(width, ui.spacing().interact_size.y),
        layout,
        |ui| {
            ui.set_min_width(width);
            ui.add(egui::Label::new(text).truncate());
        },
    );
}

/// Hover text: who owns the process and the chain of processes that started it.
fn details(ui: &mut Ui, p: &ProcInfo, by_pid: &HashMap<u32, &ProcInfo>, now: u64) {
    ui.set_max_width(520.0);
    ui.strong(format!("{} (pid {})", p.name, p.pid));

    // Oldest ancestor first, ending with this process.
    let mut chain = vec![p.name.as_str()];
    let mut current = p;
    while let Some(parent) = current.parent.and_then(|pid| by_pid.get(&pid)) {
        if parent.pid == current.pid || chain.len() > 64 {
            break;
        }
        chain.push(&parent.name);
        current = parent;
    }
    chain.reverse();

    egui::Grid::new("process_details")
        .num_columns(2)
        .show(ui, |ui| {
            let mut field = |name: &str, value: &str| {
                if !value.is_empty() {
                    ui.weak(name);
                    ui.add(egui::Label::new(value).wrap());
                    ui.end_row();
                }
            };
            field("Started by", &chain.join(" › "));
            field("User", &p.user);
            field("Belongs to", &p.group);
            field("Unit", &p.unit);
            field(
                "Running for",
                &fmt_duration(now.saturating_sub(p.start_time)),
            );
            field("Command", &p.cmd);
        });
}

pub fn fmt_duration(secs: u64) -> String {
    let (days, hours, minutes) = (secs / 86_400, secs / 3_600 % 24, secs / 60 % 60);
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {}s", secs % 60)
    } else {
        format!("{secs}s")
    }
}
