use std::collections::VecDeque;

/// Number of samples kept for the history graphs (one per second).
pub const HISTORY_LEN: usize = 60;

/// A group of same-named processes and what they consume.
/// The meaning of `values` depends on the panel that owns the list.
#[derive(Clone, Debug, Default)]
pub struct TopProc {
    pub name: String,
    pub count: usize,
    pub values: [f64; 2],
}

#[derive(Clone, Debug, Default)]
pub struct CpuInfo {
    pub name: String,
    /// Percent of all cores, 0-100.
    pub usage: f32,
    pub per_core: Vec<f32>,
    pub temp: Option<f32>,
    /// values[0] = percent of all cores.
    pub top: Vec<TopProc>,
}

/// How badly a shortage of memory is holding programs up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PressureLevel {
    #[default]
    Normal,
    Warning,
    Critical,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MemPressure {
    /// Percent of RAM that programs could still get without anything being swapped out.
    pub available_pct: f32,
    /// Percent of the last 10 seconds in which at least one task was stalled
    /// waiting for memory. None when the kernel has no pressure stall information.
    pub stall: Option<f32>,
    pub level: PressureLevel,
}

#[derive(Clone, Debug, Default)]
pub struct MemInfo {
    pub total: u64,
    pub used: u64,
    pub swap_total: u64,
    pub swap_used: u64,
    pub pressure: MemPressure,
    pub dimm_temps: Vec<f32>,
    /// values[0] = resident bytes.
    pub top: Vec<TopProc>,
}

#[derive(Clone, Debug, Default)]
pub struct GpuInfo {
    pub name: String,
    pub util: Option<f32>,
    pub vram_used: u64,
    pub vram_total: u64,
    pub temp: Option<f32>,
    pub power_w: Option<f32>,
    pub fan_pct: Option<u32>,
    /// values[0] = VRAM bytes, values[1] = GPU percent. None if the driver can't tell us.
    pub top: Option<Vec<TopProc>>,
}

#[derive(Clone, Debug, Default)]
pub struct Drive {
    pub name: String,
    pub model: String,
    pub size: u64,
    pub temp: Option<f32>,
    /// Bytes per second.
    pub read_rate: f64,
    pub write_rate: f64,
}

#[derive(Clone, Debug, Default)]
pub struct Filesystem {
    pub mount: String,
    pub fs_type: String,
    pub total: u64,
    pub used: u64,
}

#[derive(Clone, Debug, Default)]
pub struct StorageInfo {
    pub drives: Vec<Drive>,
    pub filesystems: Vec<Filesystem>,
    /// values[0] = read bytes/s, values[1] = written bytes/s.
    pub top: Vec<TopProc>,
}

#[derive(Clone, Debug, Default)]
pub struct NetIface {
    pub name: String,
    pub up: bool,
    pub temp: Option<f32>,
    /// Bytes per second.
    pub down_rate: f64,
    pub up_rate: f64,
}

#[derive(Clone, Debug, Default)]
pub struct PingTarget {
    pub label: String,
    pub host: String,
    /// None when the last ping timed out.
    pub ms: Option<f32>,
}

#[derive(Clone, Debug, Default)]
pub struct NetInfo {
    pub ifaces: Vec<NetIface>,
    pub pings: Vec<PingTarget>,
    /// values[0] = download bytes/s, values[1] = upload bytes/s. TCP only.
    pub top: Vec<TopProc>,
}

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub cpu: CpuInfo,
    pub mem: MemInfo,
    pub gpus: Vec<GpuInfo>,
    pub storage: StorageInfo,
    pub net: NetInfo,
}

#[derive(Clone, Debug, Default)]
pub struct History {
    pub cpu: VecDeque<f64>,
    pub mem_pressure: VecDeque<(f64, PressureLevel)>,
    pub gpus: Vec<VecDeque<f64>>,
    pub disk_read: VecDeque<f64>,
    pub disk_write: VecDeque<f64>,
    pub net_down: VecDeque<f64>,
    pub net_up: VecDeque<f64>,
}

impl History {
    pub fn push(&mut self, s: &Snapshot) {
        push(&mut self.cpu, s.cpu.usage as f64);
        let pressure = s.mem.pressure;
        // Plotted as memory that is spoken for, so the graph rises as room runs out.
        push(&mut self.mem_pressure, (100.0 - pressure.available_pct as f64, pressure.level));
        self.gpus.resize_with(s.gpus.len(), VecDeque::new);
        for (h, g) in self.gpus.iter_mut().zip(&s.gpus) {
            push(h, g.util.unwrap_or(0.0) as f64);
        }
        push(&mut self.disk_read, s.storage.drives.iter().map(|d| d.read_rate).sum());
        push(&mut self.disk_write, s.storage.drives.iter().map(|d| d.write_rate).sum());
        push(&mut self.net_down, s.net.ifaces.iter().map(|i| i.down_rate).sum());
        push(&mut self.net_up, s.net.ifaces.iter().map(|i| i.up_rate).sum());
    }
}

fn push<T>(q: &mut VecDeque<T>, v: T) {
    if q.len() == HISTORY_LEN {
        q.pop_front();
    }
    q.push_back(v);
}

/// What the sampler thread shares with the UI.
#[derive(Clone, Debug, Default)]
pub struct State {
    pub snapshot: Snapshot,
    pub history: History,
}
