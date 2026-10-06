use std::collections::HashMap;

use nvml_wrapper::Nvml;
use nvml_wrapper::enum_wrappers::device::TemperatureSensor;
use nvml_wrapper::enums::device::UsedGpuMemory;use crate::snapshot::GpuInfo;

/// How far back to ask NVML for per-process utilization samples, in microseconds.
/// Wider than the 1s refresh so a process doesn't flicker to 0% between samples.
const UTIL_WINDOW_US: u64 = 2_000_000;

pub struct Nvidia {
    nvml: Nvml,
    /// Newest per-process sample timestamp seen for each device (NVML's own clock).
    newest_sample: Vec<u64>,
}

impl Nvidia {
    /// None when there is no NVIDIA driver on this machine.
    pub fn new() -> Option<Self> {
        let nvml = Nvml::init().ok()?;
        let count = nvml.device_count().ok()? as usize;
        Some(Self { nvml, newest_sample: vec![0; count] })
    }

    pub fn sample(&mut self) -> Vec<GpuInfo> {
        (0..self.newest_sample.len()).filter_map(|i| self.sample_device(i)).collect()
    }

    fn sample_device(&mut self, index: usize) -> Option<GpuInfo> {
        let dev = self.nvml.device_by_index(index as u32).ok()?;
        let mem = dev.memory_info().ok();

        // pid -> VRAM bytes. A process can appear in both lists.
        let mut vram: HashMap<u32, u64> = HashMap::new();
        let lists = [dev.running_graphics_processes(), dev.running_compute_processes()];
        for p in lists.into_iter().flatten().flatten() {
            let bytes = match p.used_gpu_memory {
                UsedGpuMemory::Used(b) => b,
                UsedGpuMemory::Unavailable => 0,
            };
            let entry = vram.entry(p.pid).or_default();
            *entry = (*entry).max(bytes);
        }

        // pid -> (timestamp, GPU percent), keeping the newest sample per process.
        // Errors here just mean "no samples in the window".
        let mut util: HashMap<u32, (u64, u32)> = HashMap::new();
        let since = self.newest_sample[index].saturating_sub(UTIL_WINDOW_US);
        for s in dev.process_utilization_stats(since).unwrap_or_default() {
            self.newest_sample[index] = self.newest_sample[index].max(s.timestamp);
            let entry = util.entry(s.pid).or_default();
            if s.timestamp >= entry.0 {
                *entry = (s.timestamp, s.sm_util);
            }
        }

        let procs = vram
            .iter()
            .map(|(pid, bytes)| (*pid, [*bytes as f64, util.get(pid).map_or(0, |u| u.1) as f64]))
            .collect();

        Some(GpuInfo {
            name: dev.name().unwrap_or_else(|_| format!("NVIDIA GPU {index}")),
            util: dev.utilization_rates().ok().map(|u| u.gpu as f32),
            vram_used: mem.as_ref().map_or(0, |m| m.used),
            vram_total: mem.as_ref().map_or(0, |m| m.total),
            temp: dev.temperature(TemperatureSensor::Gpu).ok().map(|t| t as f32),
            power_w: dev.power_usage().ok().map(|mw| mw as f32 / 1000.0),
            fan_pct: dev.fan_speed(0).ok(),
            procs,
        })
    }
}
