use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use sysinfo::Disks;

use super::temps;
use crate::snapshot::{Drive, Filesystem, StorageInfo};

/// /sys/block/*/stat counts in 512-byte sectors regardless of the device's block size.
const SECTOR: u64 = 512;

struct PhysicalDrive {
    name: String,
    model: String,
    size: u64,
    stat: PathBuf,
    hwmon: Option<PathBuf>,
}

pub struct Storage {
    drives: Vec<PhysicalDrive>,
    /// Sectors (read, written) per drive at the previous sample.
    last: HashMap<String, (u64, u64)>,
    disks: Disks,
}

impl Storage {
    pub fn new() -> Self {
        // Only real hardware has a `device` link; this skips zram, loop and dm devices.
        let mut drives: Vec<PhysicalDrive> = temps::subdirs("/sys/block")
            .into_iter()
            .filter(|b| b.join("device").exists())
            .filter_map(|b| {
                let size = temps::read_u64(b.join("size"))? * SECTOR;
                (size > 0).then(|| PhysicalDrive {
                    name: b
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    model: temps::read_string(b.join("device/model")).unwrap_or_default(),
                    size,
                    stat: b.join("stat"),
                    hwmon: temps::hwmon_under(&b.join("device")),
                })
            })
            .collect();
        drives.sort_by(|a, b| a.name.cmp(&b.name));
        let last = drives
            .iter()
            .filter_map(|d| Some((d.name.clone(), sectors(&d.stat)?)))
            .collect();
        Self {
            drives,
            last,
            disks: Disks::new_with_refreshed_list(),
        }
    }

    /// `secs` is the time since the previous sample.
    pub fn sample(&mut self, secs: f64) -> StorageInfo {
        let drives = self
            .drives
            .iter()
            .map(|d| {
                let now = sectors(&d.stat).unwrap_or_default();
                let prev = self.last.insert(d.name.clone(), now).unwrap_or(now);
                let rate = |n: u64, p: u64| (n.saturating_sub(p) * SECTOR) as f64 / secs;
                Drive {
                    name: d.name.clone(),
                    model: d.model.clone(),
                    size: d.size,
                    temp: d.hwmon.as_deref().and_then(temps::first_temp),
                    read_rate: rate(now.0, prev.0),
                    write_rate: rate(now.1, prev.1),
                }
            })
            .collect();

        StorageInfo {
            drives,
            filesystems: self.filesystems(),
        }
    }

    fn filesystems(&mut self) -> Vec<Filesystem> {
        self.disks.refresh(true);
        let mut list: Vec<_> = self.disks.list().iter().collect();
        // A btrfs volume shows up once per mounted subvolume (/ and /home);
        // keep only the shortest mount point for each device.
        list.sort_by_key(|d| d.mount_point().as_os_str().len());
        let mut seen = HashSet::new();
        let mut out: Vec<Filesystem> = list
            .into_iter()
            .filter(|d| d.total_space() > 0 && seen.insert(d.name().to_os_string()))
            .map(|d| Filesystem {
                mount: d.mount_point().to_string_lossy().into_owned(),
                fs_type: d.file_system().to_string_lossy().into_owned(),
                total: d.total_space(),
                used: d.total_space().saturating_sub(d.available_space()),
            })
            .collect();
        out.sort_by(|a, b| a.mount.cmp(&b.mount));
        out
    }
}

/// (sectors read, sectors written) since boot.
fn sectors(stat: &Path) -> Option<(u64, u64)> {
    let text = temps::read_string(stat)?;
    let mut fields = text.split_whitespace();
    let read = fields.nth(2)?.parse().ok()?;
    let written = fields.nth(3)?.parse().ok()?;
    Some((read, written))
}
