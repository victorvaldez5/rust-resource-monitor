//! AMD GPUs via the amdgpu driver's sysfs files. No per-process data here;
//! that would need parsing /proc/<pid>/fdinfo.

use std::path::PathBuf;

use super::temps;
use crate::snapshot::GpuInfo;

const AMD_VENDOR: &str = "0x1002";

pub struct AmdGpu {
    name: String,
    /// /sys/class/drm/cardN/device
    device: PathBuf,
    hwmon: Option<PathBuf>,
}

pub fn discover() -> Vec<AmdGpu> {
    let mut cards: Vec<PathBuf> = temps::subdirs("/sys/class/drm")
        .into_iter()
        // "card1" is a GPU; "card1-DP-1" is one of its connectors.
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("card") && !n.contains('-'))
        })
        .collect();
    cards.sort();
    cards
        .into_iter()
        .map(|card| card.join("device"))
        .filter(|dev| temps::read_string(dev.join("vendor")).as_deref() == Some(AMD_VENDOR))
        .map(|device| AmdGpu {
            name: model_name(&device),
            hwmon: temps::hwmon_under(&device),
            device,
        })
        .collect()
}

impl AmdGpu {
    pub fn sample(&self) -> GpuInfo {
        let read = |file: &str| temps::read_u64(self.device.join(file));
        GpuInfo {
            name: self.name.clone(),
            util: read("gpu_busy_percent").map(|v| v as f32),
            vram_used: read("mem_info_vram_used").unwrap_or(0),
            vram_total: read("mem_info_vram_total").unwrap_or(0),
            temp: self.hwmon.as_deref().and_then(temps::first_temp),
            power_w: None,
            fan_pct: None,
            procs: Default::default(),
        }
    }
}

/// Looks the PCI device id up in the system's pci.ids database.
fn model_name(device: &std::path::Path) -> String {
    let fallback = "AMD Radeon".to_string();
    let Some(id) = temps::read_string(device.join("device")) else {
        return fallback;
    };
    let id = id.trim_start_matches("0x").to_lowercase();
    let Some(db) = ["/usr/share/hwdata/pci.ids", "/usr/share/misc/pci.ids"]
        .iter()
        .find_map(|p| std::fs::read_to_string(p).ok())
    else {
        return fallback;
    };
    // Vendor lines start in column 0; that vendor's devices follow, indented by one tab.
    db.lines()
        .skip_while(|l| !l.starts_with("1002"))
        .skip(1)
        .take_while(|l| l.starts_with('\t') || l.starts_with('#'))
        .find_map(|l| l.strip_prefix('\t')?.strip_prefix(id.as_str()))
        .map(|name| {
            // "Raphael [Radeon 610M]" -> "AMD Radeon 610M"
            let name = name.trim();
            match (name.find('['), name.rfind(']')) {
                (Some(a), Some(b)) if a < b => format!("AMD {}", &name[a + 1..b]),
                _ => format!("AMD {name}"),
            }
        })
        .unwrap_or(fallback)
}
