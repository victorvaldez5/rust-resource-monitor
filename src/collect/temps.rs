//! Helpers for reading temperatures from /sys/class/hwmon.
//!
//! hwmonN numbers change between boots, so sensors are always found by driver
//! name or by the device they hang off, never by index.

use std::fs;
use std::path::{Path, PathBuf};

pub fn read_string(path: impl AsRef<Path>) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

pub fn read_u64(path: impl AsRef<Path>) -> Option<u64> {
    read_string(path)?.parse().ok()
}

/// Reads a millidegree file and returns degrees Celsius.
fn read_milli(path: impl AsRef<Path>) -> Option<f32> {
    read_string(path)?.parse::<f32>().ok().map(|v| v / 1000.0)
}

/// All hwmon directories whose driver name matches, in a stable order.
pub fn hwmon_named(name: &str) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = subdirs("/sys/class/hwmon")
        .into_iter()
        .filter(|d| read_string(d.join("name")).as_deref() == Some(name))
        .collect();
    dirs.sort();
    dirs
}

/// The hwmon directory belonging to a device, e.g. /sys/block/nvme0n1/device.
/// Handles both `device/hwmonN` (NVMe) and `device/hwmon/hwmonN` (most others).
pub fn hwmon_under(device: &Path) -> Option<PathBuf> {
    let is_hwmon = |p: &PathBuf| {
        p.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("hwmon") && n.len() > 5)
    };
    subdirs(device)
        .into_iter()
        .chain(subdirs(device.join("hwmon")))
        .find(is_hwmon)
}

pub fn first_temp(hwmon: &Path) -> Option<f32> {
    read_milli(hwmon.join("temp1_input"))
}

pub fn temp_by_label(hwmon: &Path, label: &str) -> Option<f32> {
    let entries = fs::read_dir(hwmon).ok()?;
    for entry in entries.flatten() {
        let file = entry.file_name();
        let Some(prefix) = file.to_str().and_then(|f| f.strip_suffix("_label")) else {
            continue;
        };
        if prefix.starts_with("temp") && read_string(entry.path()).as_deref() == Some(label) {
            return read_milli(hwmon.join(format!("{prefix}_input")));
        }
    }
    None
}

pub fn subdirs(dir: impl AsRef<Path>) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect()
        })
        .unwrap_or_default()
}
