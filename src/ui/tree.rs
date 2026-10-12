//! The process tree: every process under the one that started it.

use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

use eframe::egui::{self, Align, Layout, RichText, Sense, Ui};

use super::panel::{explanation, fmt_bytes, trust_color};
use crate::explain::group_key;
use crate::security::Trust;
use crate::snapshot::{ProcInfo, ProcUsage};

const INDENT: f32 = 16.0;
/// Widths of the fixed columns to the right of the name.
const PID_WIDTH: f32 = 64.0;
const USER_WIDTH: f32 = 96.0;
const GROUP_WIDTH: f32 = 120.0;
const CPU_WIDTH: f32 = 60.0;
const MEM_WIDTH: f32 = 84.0;
/// The name column never gets narrower than this; the list scrolls sideways instead.
const NAME_MIN_WIDTH: f32 = 160.0;

/// What the user has folded, opened and picked in the tree.
#[derive(Default)]
pub struct TreeState {
    /// Pids whose children are hidden.
    collapsed: HashSet<u32>,
    /// Per-core groups that are open, as (parent pid, group name). Groups start closed.
    open_groups: HashSet<(u32, String)>,
    /// The process shown in the details pane.
    selected: Option<u32>,
}

impl TreeState {
    /// `collapsed` are the pids that start out folded away.
    pub fn new(collapsed: impl IntoIterator<Item = u32>) -> Self {
        Self {
            collapsed: collapsed.into_iter().collect(),
            open_groups: HashSet::new(),
            selected: None,
        }
    }
}

/// Several siblings that are copies of one thing, such as `ksoftirqd/0`, `ksoftirqd/1`, ...
pub struct Group {
    pub name: String,
    pub members: Vec<usize>,
    open: bool,
}

/// One visible line of the tree.
pub struct Row {
    /// Index into the process list; for a group, its first member.
    pub index: usize,
    pub depth: usize,
    pub children: usize,
    /// Set when the line stands for several processes.
    pub group: Option<Group>,
}

enum Entry {
    One(usize),
    Group(String, Vec<usize>),
}

/// Folds siblings that share a name before the first '/' into one entry each,
/// at the place the first of them appeared. A lone one stays a plain process.
fn entries(procs: &[ProcInfo], siblings: &[usize]) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::new();
    let mut at: HashMap<&str, usize> = HashMap::new();
    for &i in siblings {
        let name = procs[i].name.as_str();
        let key = group_key(name);
        if key == name {
            out.push(Entry::One(i));
        } else if let Some(&slot) = at.get(key) {
            if let Entry::Group(_, members) = &mut out[slot] {
                members.push(i);
            }
        } else {
            at.insert(key, out.len());
            out.push(Entry::Group(key.to_string(), vec![i]));
        }
    }
    for e in &mut out {
        if let Entry::Group(_, members) = e
            && members.len() == 1
        {
            *e = Entry::One(members[0]);
        }
    }
    out
}

/// Flattens the tree into the lines to draw, skipping what's under a collapsed process.
/// `procs` must be ordered by pid; siblings come out in the same order.
pub fn rows(procs: &[ProcInfo], state: &TreeState) -> Vec<Row> {
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

    // (what to draw, depth, pid of the parent)
    let mut out = Vec::new();
    let mut stack: Vec<(Entry, usize, u32)> = entries(procs, &roots)
        .into_iter()
        .rev()
        .map(|e| (e, 0, 0))
        .collect();
    while let Some((entry, depth, parent)) = stack.pop() {
        match entry {
            Entry::One(index) => {
                let kids = children
                    .get(&procs[index].pid)
                    .map_or(&[][..], Vec::as_slice);
                out.push(Row {
                    index,
                    depth,
                    children: kids.len(),
                    group: None,
                });
                if !state.collapsed.contains(&procs[index].pid) {
                    let pid = procs[index].pid;
                    let kids = entries(procs, kids);
                    stack.extend(kids.into_iter().rev().map(|e| (e, depth + 1, pid)));
                }
            }
            Entry::Group(name, members) => {
                let open = state.open_groups.contains(&(parent, name.clone()));
                out.push(Row {
                    index: members[0],
                    depth,
                    children: members.len(),
                    group: Some(Group {
                        name,
                        members: members.clone(),
                        open,
                    }),
                });
                if open {
                    stack.extend(
                        members
                            .into_iter()
                            .rev()
                            .map(|i| (Entry::One(i), depth + 1, parent)),
                    );
                }
            }
        }
    }
    out
}

/// A group's line shows the sum of its members.
fn group_usage(procs: &[ProcInfo], members: &[usize]) -> (ProcUsage, String, String) {
    let first = &procs[members[0]];
    let mut usage = ProcUsage::default();
    let (mut user, mut group) = (first.user.clone(), first.group.clone());
    for &i in members {
        usage.cpu += procs[i].usage.cpu;
        usage.mem += procs[i].usage.mem;
        if procs[i].user != user {
            user = "various".to_string();
        }
        if procs[i].group != group {
            group = "various".to_string();
        }
    }
    (usage, user, group)
}

pub fn show(ui: &mut Ui, procs: &[ProcInfo], state: &mut TreeState) {
    let by_pid: HashMap<u32, &ProcInfo> = procs.iter().map(|p| (p.pid, p)).collect();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    if state.selected.is_some()
        && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        state.selected = None;
    }
    if let Some(pid) = state.selected {
        // Docked before the list so the list gets what is left.
        egui::Panel::right("process_pane")
            .default_size(380.0)
            .size_range(260.0..=700.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Plain text and on the left: a glyph at the far edge can go unseen.
                    if ui.button("Close").on_hover_text("Esc").clicked() {
                        state.selected = None;
                    }
                    ui.weak("Process details");
                });
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| match by_pid.get(&pid) {
                    Some(p) => details(ui, p, &by_pid, now),
                    None => {
                        ui.label(format!("Process {pid} has exited."));
                    }
                });
            });
    }
    ui.horizontal(|ui| {
        ui.weak(format!("{} processes", procs.len()));
        if ui.small_button("Expand all").clicked() {
            state.collapsed.clear();
        }
        if ui.small_button("Collapse all").clicked() {
            state.collapsed.extend(procs.iter().map(|p| p.pid));
        }
    });

    let rows = rows(procs, state);
    let height = ui.spacing().interact_size.y;
    // Narrow windows, or an open pane, scroll sideways rather than cutting columns off.
    let fixed = PID_WIDTH + USER_WIDTH + GROUP_WIDTH + CPU_WIDTH + MEM_WIDTH;
    let min_width = fixed + NAME_MIN_WIDTH + ui.spacing().item_spacing.x * 6.0;
    let width = ui.available_width().max(min_width);
    // A bar that is always there, so it is clear the list can be moved sideways
    // (Shift + mouse wheel also does it).
    ui.style_mut().spacing.scroll = egui::style::ScrollStyle::solid();
    egui::ScrollArea::horizontal()
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .show(ui, |ui| {
            ui.set_width(width);
            let header =
                ["PID", "User", "Belongs to", "CPU", "Memory"].map(|h| RichText::new(h).weak());
            line(ui, header, |ui| {
                ui.weak("Process");
            });
            ui.separator();
            // Only the lines scrolled into view are laid out.
            egui::ScrollArea::vertical().auto_shrink(false).show_rows(
                ui,
                height,
                rows.len(),
                |ui, range| {
                    for row in &rows[range] {
                        match &row.group {
                            Some(group) => group_row(ui, procs, row, group, state),
                            None => process_row(ui, procs, row, state, &by_pid, now),
                        }
                    }
                },
            );
        });
}

fn process_row(
    ui: &mut Ui,
    procs: &[ProcInfo],
    row: &Row,
    state: &mut TreeState,
    by_pid: &HashMap<u32, &ProcInfo>,
    now: u64,
) {
    let p = &procs[row.index];
    let cells = [
        RichText::new(p.pid.to_string()).monospace(),
        RichText::new(&p.user),
        RichText::new(&p.group),
        RichText::new(format!("{:.1}%", p.usage.cpu)).monospace(),
        RichText::new(fmt_bytes(p.usage.mem as f64)).monospace(),
    ];
    let area = line(ui, cells, |ui| {
        ui.add_space(row.depth as f32 * INDENT);
        let arrow = match (row.children, state.collapsed.contains(&p.pid)) {
            (0, _) => "    ",
            (_, true) => "⏵ ",
            (_, false) => "⏷ ",
        };
        // The arrow folds the children; the name selects the process.
        let arrow = ui.add(egui::Label::new(arrow).sense(Sense::click()));
        if arrow.clicked() && row.children > 0 && !state.collapsed.remove(&p.pid) {
            state.collapsed.insert(p.pid);
        }
        if !matches!(p.trust, Trust::Verified | Trust::Kernel) {
            ui.label(RichText::new("●").color(trust_color(p.trust)))
                .on_hover_text(p.trust.label());
        }
        let name = RichText::new(&p.name);
        let name = if state.selected == Some(p.pid) {
            name.strong().underline()
        } else {
            name
        };
        let label = egui::Label::new(name).truncate().sense(Sense::click());
        let response = ui.add(label).on_hover_ui(|ui| details(ui, p, by_pid, now));
        if response.clicked() {
            // Clicking the selected process again closes the pane.
            state.selected = (state.selected != Some(p.pid)).then_some(p.pid);
        }
    });
    // Outline the process shown in the details pane.
    if state.selected == Some(p.pid) {
        ui.painter().rect_stroke(
            area.shrink(1.0),
            3.0,
            ui.visuals().selection.stroke,
            egui::StrokeKind::Inside,
        );
    }
}

/// A line standing for per-core copies of one process; clicking it opens them.
fn group_row(ui: &mut Ui, procs: &[ProcInfo], row: &Row, group: &Group, state: &mut TreeState) {
    let (usage, user, owner) = group_usage(procs, &group.members);
    let cells = [
        RichText::new(""),
        RichText::new(user),
        RichText::new(&owner),
        RichText::new(format!("{:.1}%", usage.cpu)).monospace(),
        RichText::new(fmt_bytes(usage.mem as f64)).monospace(),
    ];
    line(ui, cells, |ui| {
        ui.add_space(row.depth as f32 * INDENT);
        let arrow = if group.open { "⏷ " } else { "⏵ " };
        let text = format!("{arrow}{} ({})", group.name, group.members.len());
        let label = egui::Label::new(text).truncate().sense(Sense::click());
        let response = ui.add(label).on_hover_ui(|ui| {
            ui.strong(format!("{} ({} copies)", group.name, group.members.len()));
            ui.label("One per CPU core or device. Click to list them.");
            explanation(ui, &group.name, &owner, "");
        });
        if response.clicked() {
            let parent = procs[row.index].parent.unwrap_or(0);
            let key = (parent, group.name.clone());
            if !state.open_groups.remove(&key) {
                state.open_groups.insert(key);
            }
        }
    });
}

/// One line: fixed-width columns on the right, and `name` in whatever is left.
/// Returns the area the line covers, across the full width of the list.
fn line(ui: &mut Ui, cells: [RichText; 5], name: impl FnOnce(&mut Ui)) -> egui::Rect {
    let [pid, user, group, cpu, mem] = cells;
    let row = ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            cell(ui, MEM_WIDTH, Layout::right_to_left(Align::Center), mem);
            cell(ui, CPU_WIDTH, Layout::right_to_left(Align::Center), cpu);
            cell(ui, GROUP_WIDTH, Layout::left_to_right(Align::Center), group);
            cell(ui, USER_WIDTH, Layout::left_to_right(Align::Center), user);
            cell(ui, PID_WIDTH, Layout::right_to_left(Align::Center), pid);
            ui.with_layout(Layout::left_to_right(Align::Center), name);
        });
    });
    egui::Rect::from_x_y_ranges(ui.max_rect().x_range(), row.response.rect.y_range())
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
    if let Some(v) = &p.provenance {
        ui.separator();
        ui.label(
            RichText::new(p.trust.label())
                .strong()
                .color(trust_color(p.trust)),
        );
        ui.add(egui::Label::new(&v.reason).wrap());
        if let Some(hash) = &v.sha256 {
            ui.horizontal(|ui| {
                ui.weak("SHA-256");
                ui.add(egui::Label::new(RichText::new(hash).monospace().small()).selectable(true));
            });
        }
    }
    explanation(ui, &p.name, &p.group, &p.unit_description);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(pid: u32, parent: Option<u32>, name: &str) -> ProcInfo {
        ProcInfo {
            pid,
            parent,
            name: name.to_string(),
            ..Default::default()
        }
    }

    fn kernel() -> Vec<ProcInfo> {
        vec![
            proc(2, None, "kthreadd"),
            proc(3, Some(2), "ksoftirqd/0"),
            proc(4, Some(2), "rcu_gp"),
            proc(5, Some(2), "ksoftirqd/1"),
            proc(6, Some(2), "migration/0"),
        ]
    }

    #[test]
    fn numbered_siblings_fold_into_one_closed_group() {
        let procs = kernel();
        let rows = rows(&procs, &TreeState::default());
        // kthreadd, ksoftirqd group, rcu_gp, and migration alone (one copy stays plain).
        assert_eq!(rows.len(), 4);
        let group = rows[1].group.as_ref().unwrap();
        assert_eq!((group.name.as_str(), group.members.len()), ("ksoftirqd", 2));
        assert!(rows[3].group.is_none());
    }

    #[test]
    fn opening_a_group_lists_its_members() {
        let procs = kernel();
        let mut state = TreeState::default();
        state.open_groups.insert((2, "ksoftirqd".to_string()));
        let rows = rows(&procs, &state);
        assert_eq!(rows.len(), 6);
        assert_eq!(procs[rows[2].index].name, "ksoftirqd/0");
        assert_eq!(rows[2].depth, 2);
    }

    #[test]
    fn collapsed_parent_hides_its_groups() {
        let procs = kernel();
        let state = TreeState::new([2]);
        assert_eq!(rows(&procs, &state).len(), 1);
    }
}

#[cfg(test)]
mod pane_tests {
    use super::*;
    use eframe::egui::{Event, Key, Modifiers, Pos2, RawInput, Rect};

    fn procs() -> Vec<ProcInfo> {
        (1..5)
            .map(|pid| ProcInfo {
                pid,
                name: format!("proc{pid}"),
                ..Default::default()
            })
            .collect()
    }

    /// Runs one frame of the tree in a 900x600 window.
    fn frame(ctx: &egui::Context, state: &mut TreeState, events: Vec<Event>) {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(900.0, 600.0))),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| show(ui, &procs(), state));
        });
        out.textures_delta.clear();
    }

    #[test]
    fn escape_closes_the_pane() {
        let ctx = egui::Context::default();
        let mut state = TreeState::new([]);
        state.selected = Some(2);
        frame(&ctx, &mut state, vec![]);
        assert_eq!(state.selected, Some(2));
        let escape = Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        frame(&ctx, &mut state, vec![escape]);
        assert_eq!(state.selected, None);
    }
}
