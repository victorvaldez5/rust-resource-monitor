mod history;
mod panel;
pub mod tree;

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use eframe::egui::{self, RichText, Ui};

use crate::snapshot::State;
use panel::{BLUE, ORANGE, fmt_bytes, fmt_rate};

/// What the main area shows.
#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Hardware,
    History,
    Tree,
}

pub struct App {
    shared: Arc<Mutex<State>>,
    view: View,
    /// Pids whose children are hidden in the process tree.
    collapsed: HashSet<u32>,
    history_sort: history::Sort,
    /// Process names opened up in the history to show each pid.
    history_open: HashSet<String>,
}

impl App {
    pub fn new(shared: Arc<Mutex<State>>) -> Self {
        /// kthreadd, the parent of every kernel thread. There are hundreds
        /// of them, so they start out folded away.
        const KTHREADD: u32 = 2;
        Self { shared, view: View::Hardware, collapsed: HashSet::from([KTHREADD]), history_sort: Default::default(), history_open: HashSet::new() }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        // Copy the state out so the sampler is never blocked while we draw.
        let state = self.shared.lock().unwrap().clone();
        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.view, View::Hardware, "Hardware");
                ui.selectable_value(&mut self.view, View::History, "Process history");
                ui.selectable_value(&mut self.view, View::Tree, "Process tree");
            });
            ui.separator();
            match self.view {
                View::Hardware => {
                    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                        ui.columns(2, |cols| {
                            cpu_panel(&mut cols[0], &state);
                            mem_panel(&mut cols[0], &state);
                            net_panel(&mut cols[0], &state);
                            gpu_panel(&mut cols[1], &state);
                            storage_panel(&mut cols[1], &state);
                        });
                    });
                }
                View::History => {
                    history::show(ui, &state.snapshot.proc_log, &mut self.history_sort, &mut self.history_open);
                }
                View::Tree => tree::show(ui, &state.snapshot.procs, &mut self.collapsed),
            }
        });
    }
}

fn cpu_panel(ui: &mut Ui, state: &State) {
    let cpu = &state.snapshot.cpu;
    panel::section(ui, "CPU", &cpu.name, cpu.temp, |ui| {
        panel::usage_bar(ui, cpu.usage / 100.0, format!("{:.0}%", cpu.usage));
        panel::core_bars(ui, &cpu.per_core);
        panel::history_plot(ui, "cpu_history", &[("CPU %", &state.history.cpu, BLUE)], Some(100.0));
    });
}

fn mem_panel(ui: &mut Ui, state: &State) {
    let mem = &state.snapshot.mem;
    let hottest = mem.dimm_temps.iter().copied().reduce(f32::max);
    panel::section(ui, "RAM", "", hottest, |ui| {
        let fraction = mem.used as f32 / mem.total.max(1) as f32;
        panel::usage_bar(
            ui,
            fraction,
            format!("{} / {}", fmt_bytes(mem.used as f64), fmt_bytes(mem.total as f64)),
        );
        ui.horizontal_wrapped(|ui| {
            if mem.swap_total > 0 {
                ui.weak(format!(
                    "Swap {} / {}",
                    fmt_bytes(mem.swap_used as f64),
                    fmt_bytes(mem.swap_total as f64)
                ));
            }
            if mem.dimm_temps.len() > 1 {
                let each: Vec<String> = mem.dimm_temps.iter().map(|t| format!("{t:.0}")).collect();
                ui.weak(format!("DIMMs {} °C", each.join(" / ")));
            }
        });
        let pressure = mem.pressure;
        let stall = match pressure.stall {
            Some(stall) => format!("{stall:.1}%"),
            None => "not reported by this kernel".to_string(),
        };
        let explanation = format!(
            "Memory pressure: how hard your computer is struggling to find free memory.\n\n\
             The taller the graph, the less memory is left to hand out.\n\
             Green: plenty of room.\n\
             Yellow: getting tight. Less than 15% is left, or programs have started \
             having to wait for memory.\n\
             Red: nearly out. Less than 5% is left, or programs are waiting a lot, \
             so things will feel slow.\n\n\
             Right now {:.0}% is available, and programs spent {stall} of the last \
             10 seconds waiting for memory.",
            pressure.available_pct
        );
        ui.horizontal(|ui| {
            ui.weak("Memory pressure");
            ui.label(panel::pressure_text(pressure.level));
            ui.weak(format!("Available {:.0}%", pressure.available_pct));
            if pressure.stall.is_some() {
                ui.weak(format!("Stalled {stall}"));
            }
        })
        .response
        .on_hover_text(&explanation);
        panel::pressure_plot(ui, "mem_pressure_history", &state.history.mem_pressure)
            .on_hover_text(&explanation);
    });
}

fn gpu_panel(ui: &mut Ui, state: &State) {
    if state.snapshot.gpus.is_empty() {
        panel::section(ui, "GPU", "", None, |ui| {
            ui.weak("No supported GPU found");
        });
        return;
    }
    for (i, gpu) in state.snapshot.gpus.iter().enumerate() {
        panel::section(ui, "GPU", &gpu.name, gpu.temp, |ui| {
            let util = gpu.util.unwrap_or(0.0);
            panel::usage_bar(ui, util / 100.0, format!("{util:.0}%"));
            panel::usage_bar(
                ui,
                gpu.vram_used as f32 / gpu.vram_total.max(1) as f32,
                format!(
                    "VRAM {} / {}",
                    fmt_bytes(gpu.vram_used as f64),
                    fmt_bytes(gpu.vram_total as f64)
                ),
            );
            ui.horizontal_wrapped(|ui| {
                if let Some(w) = gpu.power_w {
                    ui.weak(format!("Power {w:.0} W"));
                }
                if let Some(fan) = gpu.fan_pct {
                    ui.weak(format!("Fan {fan}%"));
                }
            });
            if let Some(history) = state.history.gpus.get(i) {
                panel::history_plot(ui, &format!("gpu_history_{i}"), &[("GPU %", history, BLUE)], Some(100.0));
            }
        });
    }
}

fn storage_panel(ui: &mut Ui, state: &State) {
    let storage = &state.snapshot.storage;
    let hottest = storage.drives.iter().filter_map(|d| d.temp).reduce(f32::max);
    panel::section(ui, "Storage", "", hottest, |ui| {
        for drive in &storage.drives {
            let title = RichText::new(&drive.name).strong();
            let subtitle = format!("{} · {}", drive.model, fmt_bytes(drive.size as f64));
            panel::header(ui, title, &subtitle, drive.temp);
            ui.weak(format!(
                "Read {}   Write {}",
                fmt_rate(drive.read_rate),
                fmt_rate(drive.write_rate)
            ));
        }
        ui.add_space(4.0);
        for fs in &storage.filesystems {
            panel::usage_bar(
                ui,
                fs.used as f32 / fs.total.max(1) as f32,
                format!(
                    "{} ({})  {} / {}",
                    fs.mount,
                    fs.fs_type,
                    fmt_bytes(fs.used as f64),
                    fmt_bytes(fs.total as f64)
                ),
            );
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new("■ read").color(BLUE).small());
            ui.label(RichText::new("■ write").color(ORANGE).small());
        });
        panel::history_plot(
            ui,
            "disk_history",
            &[
                ("Read", &state.history.disk_read, BLUE),
                ("Write", &state.history.disk_write, ORANGE),
            ],
            // Keep the axis from zooming in on idle noise.
            Some(1024.0 * 1024.0),
        );
    });
}

fn net_panel(ui: &mut Ui, state: &State) {
    let net = &state.snapshot.net;
    let hottest = net.ifaces.iter().filter_map(|i| i.temp).reduce(f32::max);
    panel::section(ui, "Network", "", hottest, |ui| {
        if net.ifaces.is_empty() {
            ui.weak("No active connection");
        }
        for iface in &net.ifaces {
            panel::header(ui, RichText::new(&iface.name).strong(), "", iface.temp);
            ui.weak(format!(
                "Download {}   Upload {}",
                fmt_rate(iface.down_rate),
                fmt_rate(iface.up_rate)
            ));
        }
        ui.horizontal_wrapped(|ui| {
            for ping in &net.pings {
                ui.weak(format!("Ping {} ({})", ping.label.to_lowercase(), ping.host));
                match ping.ms {
                    Some(ms) => ui.label(panel::latency_text(ms)),
                    None => ui.label(RichText::new("timeout").color(panel::RED).strong()),
                };
                ui.add_space(8.0);
            }
        });
        ui.horizontal(|ui| {
            ui.label(RichText::new("■ download").color(BLUE).small());
            ui.label(RichText::new("■ upload").color(ORANGE).small());
        });
        panel::history_plot(
            ui,
            "net_history",
            &[
                ("Download", &state.history.net_down, BLUE),
                ("Upload", &state.history.net_up, ORANGE),
            ],
            // Keep the axis from zooming in on idle noise.
            Some(128.0 * 1024.0),
        );
    });
}
