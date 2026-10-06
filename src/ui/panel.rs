//! Building blocks shared by the four hardware panels.

use std::collections::VecDeque;

use eframe::egui::{self, Align, Color32, Layout, RichText, Ui};
use egui_plot::{Bar, BarChart, Line, Plot, PlotPoints};

use crate::snapshot::{HISTORY_LEN, PressureLevel};

pub const BLUE: Color32 = Color32::from_rgb(86, 156, 240);
pub const ORANGE: Color32 = Color32::from_rgb(232, 142, 62);
pub const RED: Color32 = Color32::from_rgb(226, 74, 74);
const AMBER: Color32 = Color32::from_rgb(222, 160, 50);
const GREEN: Color32 = Color32::from_rgb(84, 176, 110);

const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
const MIB: f64 = 1024.0 * 1024.0;

pub fn fmt_bytes(bytes: f64) -> String {
    if bytes >= 1024.0 * GIB {
        format!("{:.2} TiB", bytes / (1024.0 * GIB))
    } else if bytes >= GIB {
        format!("{:.1} GiB", bytes / GIB)
    } else if bytes >= MIB {
        format!("{:.0} MiB", bytes / MIB)
    } else {
        format!("{:.0} KiB", bytes / 1024.0)
    }
}

pub fn fmt_rate(bytes_per_sec: f64) -> String {
    if bytes_per_sec >= MIB {
        format!("{:.1} MiB/s", bytes_per_sec / MIB)
    } else {
        format!("{:.0} KiB/s", bytes_per_sec / 1024.0)
    }
}

/// Temperature text, coloured by how hot it is.
pub fn temp_text(celsius: f32) -> RichText {
    let color = if celsius >= 85.0 {
        RED
    } else if celsius >= 70.0 {
        AMBER
    } else {
        GREEN
    };
    RichText::new(format!("{celsius:.0} °C"))
        .color(color)
        .strong()
}

/// Ping time, coloured by how slow it is.
pub fn latency_text(ms: f32) -> RichText {
    let color = if ms >= 150.0 {
        RED
    } else if ms >= 60.0 {
        AMBER
    } else {
        GREEN
    };
    let text = if ms < 10.0 {
        format!("{ms:.1} ms")
    } else {
        format!("{ms:.0} ms")
    };
    RichText::new(text).color(color).strong()
}

/// Green, yellow and red, as in Activity Monitor's memory pressure graph.
fn pressure_color(level: PressureLevel) -> Color32 {
    match level {
        PressureLevel::Normal => GREEN,
        PressureLevel::Warning => AMBER,
        PressureLevel::Critical => RED,
    }
}

/// Memory pressure level by name, in its colour.
pub fn pressure_text(level: PressureLevel) -> RichText {
    let name = match level {
        PressureLevel::Normal => "Normal",
        PressureLevel::Warning => "Warning",
        PressureLevel::Critical => "Critical",
    };
    RichText::new(name).color(pressure_color(level)).strong()
}

/// A framed panel with a title on the left and a temperature on the right.
pub fn section(
    ui: &mut Ui,
    title: &str,
    subtitle: &str,
    temp: Option<f32>,
    body: impl FnOnce(&mut Ui),
) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_width(ui.available_width());
        header(ui, RichText::new(title).heading(), subtitle, temp);
        body(ui);
    });
    ui.add_space(6.0);
}

/// Title row: name on the left, weak subtitle next to it, temperature right-aligned.
pub fn header(ui: &mut Ui, title: RichText, subtitle: &str, temp: Option<f32>) {
    ui.horizontal(|ui| {
        ui.label(title);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if let Some(t) = temp {
                ui.label(temp_text(t));
            }
            // Whatever width is left goes to the subtitle, truncated if needed.
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.add(egui::Label::new(RichText::new(subtitle).weak()).truncate());
            });
        });
    });
}

pub fn usage_bar(ui: &mut Ui, fraction: f32, text: String) {
    ui.add(egui::ProgressBar::new(fraction.clamp(0.0, 1.0)).text(text));
}

/// A fixed, non-interactive graph of the last `HISTORY_LEN` samples.
/// `y_max` pins the top of the axis; without it the axis fits the data.
pub fn history_plot(
    ui: &mut Ui,
    id: &str,
    series: &[(&str, &VecDeque<f64>, Color32)],
    y_max: Option<f64>,
) {
    history_axes(id, y_max).show(ui, |plot_ui| {
        for (name, values, color) in series {
            // Right-align so the newest sample is always at the right edge.
            let offset = HISTORY_LEN - values.len();
            let points: Vec<[f64; 2]> = values
                .iter()
                .enumerate()
                .map(|(i, v)| [(offset + i) as f64, *v])
                .collect();
            plot_ui.line(Line::new(*name, PlotPoints::from(points)).color(*color));
        }
    });
}

/// Memory pressure history as a filled graph, each sample in its level's colour.
pub fn pressure_plot(
    ui: &mut Ui,
    id: &str,
    values: &VecDeque<(f64, PressureLevel)>,
) -> egui::Response {
    let plot = history_axes(id, Some(100.0)).show(ui, |plot_ui| {
        let offset = HISTORY_LEN - values.len();
        let bars = values
            .iter()
            .enumerate()
            // Full-width bars with no gaps read as one solid area.
            .map(|(i, (v, level))| {
                Bar::new((offset + i) as f64, *v)
                    .width(1.0)
                    .fill(pressure_color(*level))
            })
            .collect();
        // No per-bar hover text: the caller puts one tooltip on the whole graph.
        plot_ui.bar_chart(BarChart::new("Memory pressure", bars).allow_hover(false));
    });
    plot.response
}

/// The empty graph both kinds of history plot draw into.
fn history_axes(id: &str, y_max: Option<f64>) -> Plot<'_> {
    let mut plot = Plot::new(id)
        .height(64.0)
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .allow_boxed_zoom(false)
        .allow_axis_zoom_drag(false)
        .allow_double_click_reset(false)
        .show_axes([false, false])
        .show_grid([false, true])
        .show_x(false)
        .show_y(false)
        .include_x(0.0)
        .include_x((HISTORY_LEN - 1) as f64)
        .include_y(0.0);
    if let Some(max) = y_max {
        plot = plot.include_y(max);
    }
    plot
}

/// One thin vertical bar per CPU core.
pub fn core_bars(ui: &mut Ui, per_core: &[f32]) {
    if per_core.is_empty() {
        return;
    }
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 28.0), egui::Sense::hover());
    let gap = 2.0;
    let width = (rect.width() + gap) / per_core.len() as f32 - gap;
    let track = ui.visuals().extreme_bg_color;
    for (i, pct) in per_core.iter().enumerate() {
        let x = rect.left() + i as f32 * (width + gap);
        let slot =
            egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(width, rect.height()));
        ui.painter().rect_filled(slot, 1.0, track);
        let filled = rect.height() * (pct / 100.0).clamp(0.0, 1.0);
        let fill = egui::Rect::from_min_max(egui::pos2(x, rect.bottom() - filled), slot.max);
        ui.painter().rect_filled(fill, 1.0, BLUE);
    }
}
