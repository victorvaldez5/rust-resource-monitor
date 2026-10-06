mod cpu;
mod gpu_amd;
mod gpu_nvidia;
mod mem;
mod net;
mod ping;
mod procs;
mod storage;
mod temps;

use std::time::Instant;

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

use crate::snapshot::Snapshot;

/// Owns every data source and turns them into a `Snapshot` on demand.
pub struct Collector {
    sys: System,
    cpu: cpu::Cpu,
    mem: mem::Mem,
    storage: storage::Storage,
    net: net::Net,
    nvidia: Option<gpu_nvidia::Nvidia>,
    amd: Vec<gpu_amd::AmdGpu>,
    last_sample: Instant,
}

impl Collector {
    pub fn new() -> Self {
        let mut sys = System::new();
        // Loads the CPU model name; later refreshes only update usage.
        sys.refresh_cpu_all();
        let mut this = Self {
            sys,
            cpu: cpu::Cpu::new(),
            mem: mem::Mem::new(),
            storage: storage::Storage::new(),
            net: net::Net::new(),
            nvidia: gpu_nvidia::Nvidia::new(),
            amd: gpu_amd::discover(),
            last_sample: Instant::now(),
        };
        // CPU and I/O figures are deltas, so the first real sample needs a baseline.
        this.refresh_system();
        this
    }

    pub fn sample(&mut self) -> Snapshot {
        let secs = self.last_sample.elapsed().as_secs_f64().max(0.001);
        self.last_sample = Instant::now();
        self.refresh_system();

        let mut gpus = self.nvidia.as_mut().map(|n| n.sample(&self.sys)).unwrap_or_default();
        gpus.extend(self.amd.iter().map(|g| g.sample()));

        Snapshot {
            cpu: self.cpu.sample(&self.sys),
            mem: self.mem.sample(&self.sys),
            gpus,
            storage: self.storage.sample(&self.sys, secs),
            net: self.net.sample(&self.sys, secs),
        }
    }

    fn refresh_system(&mut self) {
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        self.sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing()
                .with_cpu()
                .with_memory()
                .with_disk_usage()
                .with_exe(UpdateKind::OnlyIfNotSet)
                .without_tasks(),
        );
    }
}
