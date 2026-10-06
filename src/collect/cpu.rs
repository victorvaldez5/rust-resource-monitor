use std::path::PathBuf;

use sysinfo::System;

use super::temps;
use crate::snapshot::CpuInfo;

pub struct Cpu {
    /// hwmon directory and the label of the package temperature within it.
    sensor: Option<(PathBuf, &'static str)>,
}

impl Cpu {
    pub fn new() -> Self {
        // AMD exposes k10temp/Tctl, Intel exposes coretemp/"Package id 0".
        let sensor = [("k10temp", "Tctl"), ("coretemp", "Package id 0")]
            .into_iter()
            .find_map(|(driver, label)| {
                temps::hwmon_named(driver)
                    .into_iter()
                    .next()
                    .map(|dir| (dir, label))
            });
        Self { sensor }
    }

    pub fn sample(&self, sys: &System) -> CpuInfo {
        let temp = self.sensor.as_ref().and_then(|(dir, label)| {
            temps::temp_by_label(dir, label).or_else(|| temps::first_temp(dir))
        });
        CpuInfo {
            name: sys
                .cpus()
                .first()
                .map(|c| c.brand().trim().to_string())
                .unwrap_or_default(),
            usage: sys.global_cpu_usage(),
            per_core: sys.cpus().iter().map(|c| c.cpu_usage()).collect(),
            temp,
        }
    }
}
