//! System tray icon: a live CPU and GPU gauge, a tooltip with the headline
//! numbers, and a menu to show or hide the window.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use eframe::egui::{self, ViewportCommand};
use ksni::blocking::TrayMethods;

use crate::sampler::INTERVAL;
use crate::snapshot::{PressureLevel, Snapshot, State};

/// Icon edge in pixels; the tray scales it to whatever size it draws.
const ICON_SIZE: usize = 64;

/// What the window and the tray thread share.
#[derive(Default)]
pub struct TrayState {
    /// True once the tray icon is up. Without it, closing the window quits as usual.
    pub active: AtomicBool,
    /// Whether the window is currently shown.
    visible: AtomicBool,
}

impl TrayState {
    /// Closing the window hides it instead, when there is a tray icon to bring it back.
    pub fn hide(&self, ctx: &egui::Context) {
        self.visible.store(false, Ordering::Relaxed);
        ctx.send_viewport_cmd(ViewportCommand::CancelClose);
        hide_window(ctx);
    }
}

/// Wayland can't hide a window, only minimize it, and X11 can't be relied on to
/// minimize to the tray, so ask for both.
fn hide_window(ctx: &egui::Context) {
    ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
    ctx.send_viewport_cmd(ViewportCommand::Visible(false));
}

fn show_window(ctx: &egui::Context) {
    ctx.send_viewport_cmd(ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(ViewportCommand::Focus);
}

struct Tray {
    ctx: egui::Context,
    state: Arc<TrayState>,
    gauges: Gauges,
    summary: String,
    details: String,
}

impl Tray {
    fn toggle(&self) {
        if self.state.visible.fetch_xor(true, Ordering::Relaxed) {
            hide_window(&self.ctx);
        } else {
            show_window(&self.ctx);
        }
        self.ctx.request_repaint();
    }
}

impl ksni::Tray for Tray {
    fn id(&self) -> String {
        env!("CARGO_PKG_NAME").into()
    }

    fn title(&self) -> String {
        "Resource Monitor".into()
    }

    fn category(&self) -> ksni::Category {
        ksni::Category::SystemServices
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![ksni::Icon {
            width: ICON_SIZE as i32,
            height: ICON_SIZE as i32,
            data: gauge_icon(&self.gauges),
        }]
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: self.summary.clone(),
            description: self.details.clone(),
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.toggle();
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::StandardItem;
        vec![
            StandardItem {
                label: "Show / hide window".into(),
                activate: Box::new(|this: &mut Self| this.toggle()),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Quit".into(),
                icon_name: "application-exit".into(),
                activate: Box::new(|_| std::process::exit(0)),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// Starts the tray on a background thread and keeps it in step with the samples.
/// If the desktop has no tray, the window just behaves normally.
pub fn spawn(shared: Arc<Mutex<State>>, ctx: egui::Context, state: Arc<TrayState>) {
    thread::Builder::new()
        .name("tray".into())
        .spawn(move || {
            state.visible.store(true, Ordering::Relaxed);
            let tray = Tray {
                ctx,
                state: state.clone(),
                gauges: Gauges::default(),
                summary: "Resource Monitor".into(),
                details: String::new(),
            };
            let Ok(handle) = tray.spawn() else {
                return;
            };
            state.active.store(true, Ordering::Relaxed);
            while !handle.is_closed() {
                thread::sleep(INTERVAL);
                let (gauges, details) = {
                    let s = &shared.lock().unwrap().snapshot;
                    (Gauges::from(s), details(s))
                };
                handle.update(|t| {
                    t.summary = gauges.summary();
                    t.gauges = gauges;
                    t.details = details;
                });
            }
            state.active.store(false, Ordering::Relaxed);
        })
        .expect("failed to start tray thread");
}

/// The levels the tray icon draws.
#[derive(Clone, Copy, Default)]
struct Gauges {
    /// Percent of all cores.
    cpu: f32,
    /// None when no GPU reports its load.
    gpu: Option<f32>,
    /// Percent of RAM in use.
    ram: f32,
    pressure: PressureLevel,
}

impl From<&Snapshot> for Gauges {
    fn from(s: &Snapshot) -> Self {
        Self {
            cpu: s.cpu.usage,
            gpu: s.gpus.iter().find_map(|g| g.util),
            ram: s.mem.used as f32 / s.mem.total.max(1) as f32 * 100.0,
            pressure: s.mem.pressure.level,
        }
    }
}

impl Gauges {
    fn summary(&self) -> String {
        let gpu = self
            .gpu
            .map(|g| format!(" · GPU {g:.0}%"))
            .unwrap_or_default();
        format!("CPU {:.0}%{gpu} · RAM {:.0}%", self.cpu, self.ram)
    }
}

fn details(s: &Snapshot) -> String {
    let mut lines = Vec::new();
    if let Some(t) = s.cpu.temp {
        lines.push(format!("CPU {t:.0} °C"));
    }
    let gib = |b: u64| b as f64 / (1024.0 * 1024.0 * 1024.0);
    lines.push(format!(
        "RAM {:.1} / {:.1} GiB",
        gib(s.mem.used),
        gib(s.mem.total)
    ));
    let pressure = match s.mem.pressure.level {
        PressureLevel::Normal => "normal",
        PressureLevel::Warning => "getting tight",
        PressureLevel::Critical => "critical",
    };
    lines.push(format!("Memory pressure {pressure}"));
    for g in &s.gpus {
        if let Some(t) = g.temp {
            lines.push(format!("{} {t:.0} °C", g.name));
        }
    }
    lines.join("\n")
}

/// Green, then yellow, then red as a gauge fills up.
fn load_color(pct: f32) -> [u8; 3] {
    match pct {
        p if p >= 85.0 => [0xe5, 0x48, 0x4d],
        p if p >= 60.0 => [0xf5, 0xa6, 0x23],
        _ => [0x3d, 0xd6, 0x8c],
    }
}

/// Colour of the centre dot for each memory pressure level.
fn pressure_color(level: PressureLevel) -> [u8; 3] {
    match level {
        PressureLevel::Normal => [0x3d, 0xd6, 0x8c],
        PressureLevel::Warning => [0xf5, 0xa6, 0x23],
        PressureLevel::Critical => [0xe5, 0x48, 0x4d],
    }
}

/// ARGB32 pixels for ring gauges that fill clockwise from the top: CPU on the
/// outside, then GPU if there is one, then RAM, with a dot in the middle that is
/// the memory pressure level.
fn gauge_icon(g: &Gauges) -> Vec<u8> {
    const OUTER: f32 = 31.0;
    const DOT: f32 = 5.0;
    const GAP: f32 = 1.5;
    let levels: Vec<f32> = [Some(g.cpu), g.gpu, Some(g.ram)]
        .into_iter()
        .flatten()
        .collect();
    // Equal bands from the outside in, leaving room for the dot.
    let band = (OUTER - DOT - 1.5 - GAP * (levels.len() - 1) as f32) / levels.len() as f32;
    // (inner radius, outer radius, percent) of each ring, in pixels from the centre.
    let rings: Vec<(f32, f32, f32)> = levels
        .iter()
        .enumerate()
        .map(|(i, pct)| {
            let outer = OUTER - i as f32 * (band + GAP);
            (outer - band, outer, *pct)
        })
        .collect();
    let dot = pressure_color(g.pressure);
    // Each pixel is the average of SUB x SUB samples, which smooths the edges.
    const SUB: usize = 4;
    let centre = ICON_SIZE as f32 / 2.0;
    let mut data = Vec::with_capacity(ICON_SIZE * ICON_SIZE * 4);
    for y in 0..ICON_SIZE {
        for x in 0..ICON_SIZE {
            let (mut a, mut r, mut g, mut b) = (0.0, 0.0, 0.0, 0.0);
            for sy in 0..SUB {
                for sx in 0..SUB {
                    let dx = x as f32 + (sx as f32 + 0.5) / SUB as f32 - centre;
                    let dy = y as f32 + (sy as f32 + 0.5) / SUB as f32 - centre;
                    let Some([ca, cr, cg, cb]) = ring_color(&rings, dot, dx, dy) else {
                        continue;
                    };
                    // Premultiplied, so see-through samples don't tint the edge.
                    let alpha = ca as f32 / 255.0;
                    a += alpha;
                    r += cr as f32 * alpha;
                    g += cg as f32 * alpha;
                    b += cb as f32 * alpha;
                }
            }
            let n = (SUB * SUB) as f32;
            let un = |c: f32| if a > 0.0 { (c / a).round() as u8 } else { 0 };
            data.extend_from_slice(&[(a / n * 255.0).round() as u8, un(r), un(g), un(b)]);
        }
    }
    data
}

/// The colour at a point `dx`, `dy` from the centre (the centre dot is `dot`), or None outside every ring.
fn ring_color(rings: &[(f32, f32, f32)], dot: [u8; 3], dx: f32, dy: f32) -> Option<[u8; 4]> {
    const TRACK: [u8; 4] = [0xff, 0x44, 0x47, 0x4d];
    let radius = dx.hypot(dy);
    if radius < 5.0 {
        let [r, g, b] = dot;
        return Some([0xff, r, g, b]);
    }
    let (_, _, pct) = rings
        .iter()
        .find(|(inner, outer, _)| (*inner..*outer).contains(&radius))?;
    // Fraction of the way round, clockwise from straight up.
    let turn = dx.atan2(-dy).rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU;
    if turn * 100.0 <= pct.clamp(0.0, 100.0) {
        let [r, g, b] = load_color(*pct);
        Some([0xff, r, g, b])
    } else {
        Some(TRACK)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(data: &[u8], x: usize, y: usize) -> [u8; 4] {
        let i = (y * ICON_SIZE + x) * 4;
        data[i..i + 4].try_into().unwrap()
    }

    fn gauges(cpu: f32, gpu: Option<f32>, ram: f32, pressure: PressureLevel) -> Gauges {
        Gauges {
            cpu,
            gpu,
            ram,
            pressure,
        }
    }

    #[test]
    fn icon_has_the_right_size() {
        let with_gpu = gauges(10.0, Some(20.0), 30.0, PressureLevel::Normal);
        assert_eq!(gauge_icon(&with_gpu).len(), ICON_SIZE * ICON_SIZE * 4);
        let no_gpu = gauges(10.0, None, 30.0, PressureLevel::Normal);
        assert_eq!(gauge_icon(&no_gpu).len(), ICON_SIZE * ICON_SIZE * 4);
    }

    #[test]
    fn rings_fill_clockwise_from_the_top() {
        // Centre is 32, 32. With three rings the CPU ring is the outermost band
        // (about radius 24 to 31) and the GPU ring the next one in (about 15 to 22).
        let icon = gauge_icon(&gauges(50.0, Some(0.0), 0.0, PressureLevel::Normal));
        // Just right of the top is filled; just left of the top is not.
        assert_eq!(pixel(&icon, 36, 6), [0xff, 0x3d, 0xd6, 0x8c]);
        assert_eq!(pixel(&icon, 28, 6), [0xff, 0x44, 0x47, 0x4d]);
        // The empty GPU ring shows only its track.
        assert_eq!(pixel(&icon, 36, 11), [0xff, 0x44, 0x47, 0x4d]);
        // Nothing is drawn in the corner.
        assert_eq!(pixel(&icon, 0, 0)[0], 0);
    }

    #[test]
    fn centre_dot_shows_memory_pressure() {
        for (level, color) in [
            (PressureLevel::Normal, [0x3d, 0xd6, 0x8c]),
            (PressureLevel::Warning, [0xf5, 0xa6, 0x23]),
            (PressureLevel::Critical, [0xe5, 0x48, 0x4d]),
        ] {
            let icon = gauge_icon(&gauges(0.0, None, 0.0, level));
            let [r, g, b] = color;
            assert_eq!(pixel(&icon, 32, 32), [0xff, r, g, b]);
        }
    }

    #[test]
    fn hot_load_is_red() {
        assert_eq!(load_color(95.0), [0xe5, 0x48, 0x4d]);
        assert_eq!(load_color(10.0), [0x3d, 0xd6, 0x8c]);
    }
}
