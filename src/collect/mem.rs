use std::path::PathBuf;

use sysinfo::System;

use super::temps;
use crate::snapshot::{MemInfo, MemPressure, PressureLevel};

// Pressure is warning or critical once tasks stall for more than this percent
// of the time, or less than this percent of RAM is still available.
const WARNING_STALL: f32 = 1.0;
const WARNING_AVAILABLE: f32 = 15.0;
const CRITICAL_STALL: f32 = 10.0;
const CRITICAL_AVAILABLE: f32 = 5.0;

pub struct Mem {
    /// One hwmon directory per DIMM that has a DDR5 SPD temperature sensor.
    dimms: Vec<PathBuf>,
}

impl Mem {
    pub fn new() -> Self {
        Self {
            dimms: temps::hwmon_named("spd5118"),
        }
    }

    pub fn sample(&self, sys: &System) -> MemInfo {
        MemInfo {
            total: sys.total_memory(),
            used: sys.used_memory(),
            swap_total: sys.total_swap(),
            swap_used: sys.used_swap(),
            pressure: pressure(sys),
            dimm_temps: self
                .dimms
                .iter()
                .filter_map(|d| temps::first_temp(d))
                .collect(),
        }
    }
}

fn pressure(sys: &System) -> MemPressure {
    let available_pct =
        (sys.available_memory() as f64 / sys.total_memory().max(1) as f64 * 100.0) as f32;
    let stall = stall();
    let stalled = stall.unwrap_or(0.0);
    let level = if stalled > CRITICAL_STALL || available_pct < CRITICAL_AVAILABLE {
        PressureLevel::Critical
    } else if stalled > WARNING_STALL || available_pct < WARNING_AVAILABLE {
        PressureLevel::Warning
    } else {
        PressureLevel::Normal
    };
    MemPressure {
        available_pct,
        stall,
        level,
    }
}

/// The `some avg10` figure from /proc/pressure/memory, whose first line looks
/// like `some avg10=0.00 avg60=0.00 avg300=0.00 total=4`.
fn stall() -> Option<f32> {
    let psi = temps::read_string("/proc/pressure/memory")?;
    let line = psi.lines().find(|l| l.starts_with("some"))?;
    line.split_whitespace()
        .find_map(|f| f.strip_prefix("avg10=")?.parse().ok())
}
