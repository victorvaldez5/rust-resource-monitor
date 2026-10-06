mod cpu;
mod gpu_amd;
mod gpu_nvidia;
mod mem;
mod net;
mod owner;
mod ping;
mod proc_log;
mod procs;
mod storage;
mod temps;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind, Users};

use crate::snapshot::Snapshot;

/// Owns every data source and turns them into a `Snapshot` on demand.
pub struct Collector {
    sys: System,
    users: Users,
    cpu: cpu::Cpu,
    mem: mem::Mem,
    storage: storage::Storage,
    net: net::Net,
    nvidia: Option<gpu_nvidia::Nvidia>,
    amd: Vec<gpu_amd::AmdGpu>,
    proc_log: proc_log::ProcLog,
    last_sample: Instant,
}

impl Collector {
    pub fn new() -> Self {
        let mut sys = System::new();
        // Loads the CPU model name; later refreshes only update usage.
        sys.refresh_cpu_all();
        let mut this = Self {
            sys,
            users: Users::new_with_refreshed_list(),
            cpu: cpu::Cpu::new(),
            mem: mem::Mem::new(),
            storage: storage::Storage::new(),
            net: net::Net::new(),
            nvidia: gpu_nvidia::Nvidia::new(),
            amd: gpu_amd::discover(),
            proc_log: proc_log::ProcLog::default(),
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

        let owners = owner::Owners::new(&self.sys);

        let mut gpus = self.nvidia.as_mut().map(|n| n.sample()).unwrap_or_default();
        gpus.extend(self.amd.iter().map(|g| g.sample()));

        // A process can use several GPUs; add its share on each together.
        let mut gpu_procs: HashMap<u32, [f64; 2]> = HashMap::new();
        for (pid, usage) in gpus.iter().flat_map(|g| &g.procs) {
            let sum = gpu_procs.entry(*pid).or_default();
            sum[0] += usage[0];
            sum[1] += usage[1];
        }

        let net = self.net.sample(&self.sys, secs);
        let procs = procs::list(&self.sys, &owners, &self.users, &gpu_procs, self.net.per_pid(), secs);
        let proc_log = self.proc_log.update(&procs);

        Snapshot {
            cpu: self.cpu.sample(&self.sys),
            mem: self.mem.sample(&self.sys),
            gpus,
            storage: self.storage.sample(secs),
            net,
            procs: Arc::new(procs),
            proc_log,
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
                .with_cmd(UpdateKind::OnlyIfNotSet)
                .with_user(UpdateKind::OnlyIfNotSet)
                .without_tasks(),
        );
    }
}
