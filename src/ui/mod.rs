mod panel;

use std::sync::{Arc, Mutex};

use eframe::egui::{self, RichText, Ui};

use crate::snapshot::{State, TopProc};
use panel::{BLUE, ORANGE, fmt_bytes, fmt_rate};

pub struct App {
    shared: Arc<Mutex<State>>,
}

impl App {
    pub fn new(shared: Arc<Mutex<State>>) -> Self {
        Self { shared }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        // Copy the state out so the sampler is never blocked while we draw.
        let state = self.shared.lock().unwrap().clone();
        // Side panels must be added before the central panel, which takes what's left.
        egui::Panel::right("processes").resizable(true).default_size(340.0).min_size(240.0).show(
            ui,
            |ui| {
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    processes_pane(ui, &state);
                });
            },
        );
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                ui.columns(2, |cols| {
                    cpu_panel(&mut cols[0], &state);
                    mem_panel(&mut cols[0], &state);
                    net_panel(&mut cols[0], &state);
                    gpu_panel(&mut cols[1], &state);
                    storage_panel(&mut cols[1], &state);
                });
            });
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

/// The "used by what" tables for every resource, kept apart from the hardware panels.
fn processes_pane(ui: &mut Ui, state: &State) {
    let snap = &state.snapshot;
    ui.add_space(4.0);
    ui.heading("Processes");
    ui.add_space(4.0);

    panel::subsection(ui, "CPU", |ui| {
        let rows = rows(&snap.cpu.top, |v| vec![format!("{:.1}%", v[0])]);
        panel::top_table(ui, "cpu_top", &["Process", "CPU"], &rows);
    });
    panel::subsection(ui, "RAM", |ui| {
        let rows = rows(&snap.mem.top, |v| vec![fmt_bytes(v[0])]);
        panel::top_table(ui, "mem_top", &["Process", "Memory"], &rows);
    });
    for (i, gpu) in snap.gpus.iter().enumerate() {
        // GPUs whose driver can't report per-process usage have no table to show.
        let Some(top) = &gpu.top else { continue };
        panel::subsection(ui, &format!("GPU · {}", gpu.name), |ui| {
            let rows = rows(top, |v| vec![fmt_bytes(v[0]), format!("{:.0}%", v[1])]);
            panel::top_table(ui, &format!("gpu_top_{i}"), &["Process", "VRAM", "GPU"], &rows);
        });
    }
    panel::subsection(ui, "Storage", |ui| {
        let rows = rows(&snap.storage.top, |v| vec![fmt_rate(v[0]), fmt_rate(v[1])]);
        panel::top_table(ui, "disk_top", &["Process", "Read", "Write"], &rows);
        ui.label(RichText::new("Only your own processes, unless run as root.").weak().small())
            .on_hover_text("Linux hides other users' I/O counters (/proc/<pid>/io).");
    });
    panel::subsection(ui, "Network", |ui| {
        let rows = rows(&snap.net.top, |v| vec![fmt_rate(v[0]), fmt_rate(v[1])]);
        panel::top_table(ui, "net_top", &["Process", "Down", "Up"], &rows);
        ui.label(RichText::new("TCP connections of your own processes.").weak().small())
            .on_hover_text(
                "Linux has no per-process network counters. These come from each TCP \
                 connection's byte counts, so UDP and QUIC (HTTP/3) traffic and other \
                 users' processes show up only in the unattributed row.",
            );
    });
}

/// Table rows: process name (with a count when several share it) plus formatted values.
fn rows(top: &[TopProc], format: impl Fn(&[f64; 2]) -> Vec<String>) -> Vec<Vec<String>> {
    top.iter()
        .map(|p| {
            let name = if p.count > 1 {
                format!("{} ×{}", p.name, p.count)
            } else {
                p.name.clone()
            };
            let mut row = vec![name];
            row.extend(format(&p.values));
            row
        })
        .collect()
}
