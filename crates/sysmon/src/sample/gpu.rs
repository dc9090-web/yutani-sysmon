//! AMD (amdgpu), Intel (i915, xe) and NVIDIA (nvidia) GPUs under
//! `/sys/class/drm`.
//!
//! Intel has no `gpu_busy_percent`; its busy time comes from DRM fdinfo
//! (see `fdinfo`), and an iGPU has no VRAM or temperature sensors.
//! NVIDIA's proprietary driver exposes none of these in sysfs; its
//! readings come from NVML (see `nvml`).

use std::path::{Path, PathBuf};

use common::sysfs::{read_hex, read_string, read_u64};

use super::hwmon;

pub const DRM: &str = "/sys/class/drm";
const PCI_IDS: [&str; 2] = ["/usr/share/hwdata/pci.ids", "/usr/share/misc/pci.ids"];
/// libdrm's table of exact product names by device and revision.
const AMDGPU_IDS: [&str; 2] = ["/usr/share/libdrm/amdgpu.ids", "/usr/local/share/libdrm/amdgpu.ids"];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Vendor {
    #[default]
    Amd,
    Intel,
    Nvidia,
}

/// One card with its sensor paths resolved once.
#[derive(Debug, Clone, Default)]
pub struct Card {
    pub vendor: Vendor,
    /// Kernel driver: `amdgpu`, `i915`, `xe` or `nvidia`.
    pub driver: String,
    /// PCI slot, e.g. `0000:03:00.0`.
    pub slot: String,
    pub device: PathBuf,
    pub vram_total: Option<u64>,
    pub ids: (u32, u32, u32, u32),
    pub revision: u32,
    pub busy: PathBuf,
    pub vram_used: PathBuf,
    pub runtime_status: PathBuf,
    pub clock: Option<PathBuf>,
    pub edge: Option<PathBuf>,
    pub junction: Option<PathBuf>,
    pub junction_crit: Option<u64>,
    pub power: Option<PathBuf>,
    pub power_cap: Option<PathBuf>,
    /// Intel's actual GT frequency in MHz.
    pub act_freq: Option<PathBuf>,
}

/// Every `cardN` whose driver is amdgpu, i915, xe or nvidia (connector
/// entries skipped).
pub fn discover(drm: &Path) -> Vec<Card> {
    let Ok(entries) = std::fs::read_dir(drm) else { return Vec::new() };
    let mut cards: Vec<Card> = entries
        .flatten()
        .filter(|e| {
            let n = e.file_name();
            let n = n.to_string_lossy();
            n.strip_prefix("card").is_some_and(|r| !r.is_empty() && r.bytes().all(|b| b.is_ascii_digit()))
        })
        .filter_map(|e| {
            let device = e.path().join("device");
            let driver = std::fs::read_link(device.join("driver")).ok()?;
            let driver = driver.file_name()?.to_str()?.to_owned();
            matches!(driver.as_str(), "amdgpu" | "i915" | "xe" | "nvidia").then(|| (e.path(), device, driver))
        })
        .map(|(dir, device, driver)| card(&dir, &device, driver))
        .collect();
    cards.sort_by(|a, b| a.slot.cmp(&b.slot));
    cards.dedup_by(|a, b| a.slot == b.slot);
    cards
}

fn card(dir: &Path, device: &Path, driver: String) -> Card {
    let slot = std::fs::canonicalize(device).ok().and_then(|p| p.file_name().map(|f| f.to_string_lossy().into_owned())).unwrap_or_default();
    let hw = hwmon::first_in(&device.join("hwmon"));
    let label = |kind, l| hw.as_ref().and_then(|h| hwmon::input_by_label(h, kind, l));
    let junction = label("temp", "junction");
    let in_hw = |f: &str| hw.as_ref().map(|h| h.join(f)).filter(|p| p.exists());
    let exists = |p: PathBuf| p.exists().then_some(p);
    let act_freq = match driver.as_str() {
        "i915" => exists(dir.join("gt_act_freq_mhz")).or_else(|| exists(dir.join("gt/gt0/rps_act_freq_mhz"))),
        "xe" => exists(device.join("tile0/gt0/freq0/act_freq")),
        _ => None,
    };
    Card {
        vendor: match driver.as_str() {
            "amdgpu" => Vendor::Amd,
            "nvidia" => Vendor::Nvidia,
            _ => Vendor::Intel,
        },
        driver,
        act_freq,
        vram_total: read_u64(device.join("mem_info_vram_total")),
        ids: (
            read_hex(device.join("vendor")).unwrap_or(0),
            read_hex(device.join("device")).unwrap_or(0),
            read_hex(device.join("subsystem_vendor")).unwrap_or(0),
            read_hex(device.join("subsystem_device")).unwrap_or(0),
        ),
        revision: read_hex(device.join("revision")).unwrap_or(0),
        busy: device.join("gpu_busy_percent"),
        vram_used: device.join("mem_info_vram_used"),
        runtime_status: device.join("power/runtime_status"),
        clock: label("freq", "sclk").or_else(|| in_hw("freq1_input")),
        edge: label("temp", "edge"),
        junction_crit: junction.as_ref().and_then(|j| hwmon::sibling(j, "crit")).and_then(read_u64),
        junction,
        power: in_hw("power1_average").or_else(|| in_hw("power1_input")),
        power_cap: in_hw("power1_cap"),
        slot,
        device: device.to_path_buf(),
    }
}

/// The card to show: the configured slot, else the most VRAM.
pub fn pick<'a>(cards: &'a [Card], slot: Option<&str>) -> Option<&'a Card> {
    slot.and_then(|s| cards.iter().find(|c| c.slot == s)).or_else(|| cards.iter().max_by_key(|c| c.vram_total.unwrap_or(0)))
}

/// The marketing name from `pci.ids` text: the subsystem entry, else the
/// device entry; the bracketed part when there is one.
pub fn lookup(pci_ids: &str, ids: (u32, u32, u32, u32)) -> Option<String> {
    let brand = match ids.0 {
        0x8086 => "Intel",
        0x10de => "NVIDIA",
        _ => "AMD",
    };
    lookup_raw(pci_ids, ids).map(|n| pretty(n, brand))
}

fn lookup_raw(pci_ids: &str, (ven, dev, sv, sd): (u32, u32, u32, u32)) -> Option<&str> {
    let (ven, dev, sub) = (format!("{ven:04x}"), format!("{dev:04x}"), format!("{sv:04x} {sd:04x}"));
    let mut in_vendor = false;
    let mut device_name: Option<&str> = None;
    for line in pci_ids.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if !line.starts_with('\t') {
            if device_name.is_some() {
                break;
            }
            in_vendor = line.starts_with(&ven) && line.as_bytes().get(4) == Some(&b' ');
            continue;
        }
        if !in_vendor {
            continue;
        }
        if let Some(rest) = line.strip_prefix("\t\t") {
            if device_name.is_some() && rest.starts_with(&sub) {
                return Some(rest[sub.len()..].trim());
            }
        } else if let Some(rest) = line.strip_prefix('\t') {
            if device_name.is_some() {
                break;
            }
            if rest.starts_with(&dev) && rest.as_bytes().get(4) == Some(&b' ') {
                device_name = Some(rest[4..].trim());
            }
        }
    }
    device_name
}

/// `Navi 48 [Radeon RX 9070/9070 XT]` → `AMD Radeon RX 9070/9070 XT`;
/// `Alder Lake-P GT2 [Iris Xe Graphics]` → `Intel Iris Xe Graphics`;
/// `AD104 [GeForce RTX 4070]` → `NVIDIA GeForce RTX 4070`.
fn pretty(name: &str, brand: &str) -> String {
    let inner = match (name.rfind('['), name.rfind(']')) {
        (Some(a), Some(b)) if b > a => &name[a + 1..b],
        _ => name,
    };
    let prefixed = match brand {
        "AMD" => inner.starts_with("Radeon"),
        "NVIDIA" => ["GeForce", "Quadro", "RTX", "TITAN", "Tesla"].iter().any(|p| inner.starts_with(p)),
        _ => ["UHD", "Iris", "HD Graphics", "Arc", "Graphics"].iter().any(|p| inner.starts_with(p)),
    };
    if prefixed { format!("{brand} {inner}") } else { inner.to_owned() }
}

/// The exact name from libdrm's `amdgpu.ids` (`device,\trevision,\tname`).
/// pci.ids often lists only the family, e.g. "RX 9070/9070 XT/9070 GRE";
/// the revision tells them apart.
pub fn lookup_amdgpu_ids(text: &str, device: u32, revision: u32) -> Option<String> {
    text.lines().find_map(|l| {
        let mut f = l.splitn(3, ',');
        let (d, r, n) = (f.next()?.trim(), f.next()?.trim(), f.next()?.trim());
        (u32::from_str_radix(d, 16).ok()? == device && u32::from_str_radix(r, 16).ok()? == revision && !n.is_empty()).then(|| n.to_owned())
    })
}

/// Name for a card, read once (only for the card shown): libdrm's
/// `amdgpu.ids` (AMD) or NVML (NVIDIA, unless the card is asleep), then
/// `pci.ids`, then the bare ids.
pub fn name(card: &Card) -> String {
    let exact = || match card.vendor {
        Vendor::Amd => AMDGPU_IDS.iter().find_map(|p| std::fs::read_to_string(p).ok()).and_then(|text| lookup_amdgpu_ids(&text, card.ids.1, card.revision)),
        Vendor::Nvidia if !card.suspended() => super::nvml::name(&card.slot),
        _ => None,
    };
    let family = || PCI_IDS.iter().find_map(|p| std::fs::read_to_string(p).ok()).and_then(|text| lookup(&text, card.ids));
    exact().or_else(family).unwrap_or_else(|| fallback_name(card))
}

/// `AMD GPU (1002:7550)`, for cards whose name wasn't looked up.
pub fn fallback_name(card: &Card) -> String {
    let brand = match card.vendor {
        Vendor::Amd => "AMD",
        Vendor::Intel => "Intel",
        Vendor::Nvidia => "NVIDIA",
    };
    format!("{brand} GPU ({:04x}:{:04x})", card.ids.0, card.ids.1)
}

/// The `*` line of `pp_dpm_sclk`, in MHz.
pub fn dpm_sclk_mhz(text: &str) -> Option<f32> {
    let line = text.lines().find(|l| l.trim_end().ends_with('*'))?;
    let v = line.split_whitespace().nth(1)?;
    v.trim_end_matches(|c: char| c.is_ascii_alphabetic()).parse().ok()
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Reading {
    pub busy: Option<f32>,
    pub vram_used: Option<u64>,
    pub clock_mhz: Option<f32>,
    pub edge: Option<f32>,
    pub junction: Option<f32>,
    pub power_w: Option<f32>,
    pub cap_w: Option<f32>,
    /// NVIDIA only: VRAM size, which its sysfs doesn't have.
    pub vram_total: Option<u64>,
    /// Intel only, from fdinfo: the render and video engine classes.
    pub render: Option<f32>,
    pub video: Option<f32>,
}

impl Card {
    /// A runtime-suspended dGPU reads as idle without touching its sensors
    /// (reading them would wake it).
    pub fn suspended(&self) -> bool {
        read_string(&self.runtime_status).as_deref() == Some("suspended")
    }

    /// `full`: also the popup-only readings (VRAM, clock, power). Intel's
    /// busy is left `None` for the sampler to fill in from fdinfo.
    pub fn read(&self, full: bool) -> Reading {
        if self.suspended() {
            return Reading { busy: Some(0.0), ..Default::default() };
        }
        if self.vendor == Vendor::Nvidia {
            let n = super::nvml::read(&self.slot, full);
            return Reading {
                busy: n.busy,
                edge: n.temp,
                vram_used: n.vram_used,
                vram_total: n.vram_total,
                clock_mhz: n.clock_mhz,
                power_w: n.power_w,
                cap_w: n.cap_w,
                ..Default::default()
            };
        }
        let mc = |p: &Option<PathBuf>| p.as_ref().and_then(read_u64).map(|v| v as f32 / 1000.0);
        let mut r = Reading { busy: read_u64(&self.busy).map(|v| v as f32), edge: mc(&self.edge), junction: mc(&self.junction), ..Default::default() };
        if full {
            r.vram_used = read_u64(&self.vram_used);
            r.clock_mhz = match (&self.act_freq, &self.clock) {
                (Some(p), _) => read_u64(p).map(|mhz| mhz as f32),
                (None, Some(p)) => read_u64(p).map(|hz| hz as f32 / 1e6),
                _ if self.vendor == Vendor::Amd => std::fs::read_to_string(self.device.join("pp_dpm_sclk")).ok().and_then(|t| dpm_sclk_mhz(&t)),
                _ => None,
            };
            let w = |p: &Option<PathBuf>| p.as_ref().and_then(read_u64).map(|uw| uw as f32 / 1e6);
            r.power_w = w(&self.power);
            r.cap_w = w(&self.power_cap);
        }
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDS: &str = include_str!("../../tests/fixtures/pci.ids");

    #[test]
    fn pci_names() {
        // Exact subsystem match.
        assert_eq!(lookup(IDS, (0x1002, 0x7550, 0x1da2, 0xe490)).as_deref(), Some("Sapphire Pulse Radeon RX 9070 XT"));
        // Unknown subsystem: the device entry.
        assert_eq!(lookup(IDS, (0x1002, 0x7550, 0x1da2, 0xe489)).as_deref(), Some("AMD Radeon RX 9070/9070 XT/9070 GRE"));
        assert_eq!(lookup(IDS, (0x1002, 0x744c, 0x1da2, 0x471e)).as_deref(), Some("PULSE RX 7900 XTX"));
        // Same device id under another vendor is not matched.
        assert_eq!(lookup(IDS, (0x1002, 0x15bf, 0, 0)).as_deref(), Some("Phoenix1"));
        assert_eq!(lookup(IDS, (0x1002, 0x9999, 0, 0)), None);
    }

    #[test]
    fn intel_names() {
        assert_eq!(lookup(IDS, (0x8086, 0x46a6, 0x1028, 0x0b1a)).as_deref(), Some("Intel Iris Xe Graphics"));
        assert_eq!(lookup(IDS, (0x8086, 0x3ea0, 0, 0)).as_deref(), Some("Intel UHD Graphics 620"));
        assert_eq!(lookup(IDS, (0x8086, 0x5916, 0, 0)).as_deref(), Some("Intel HD Graphics 620"));
        // Already carries the brand.
        assert_eq!(lookup(IDS, (0x8086, 0x7d55, 0, 0)).as_deref(), Some("Intel Arc Graphics"));
        let c = Card { vendor: Vendor::Intel, ids: (0x8086, 0x9a49, 0, 0), ..Default::default() };
        assert_eq!(fallback_name(&c), "Intel GPU (8086:9a49)");
    }

    #[test]
    fn nvidia_names() {
        assert_eq!(lookup(IDS, (0x10de, 0x2786, 0x10de, 0x1234)).as_deref(), Some("NVIDIA GeForce RTX 4070"));
        assert_eq!(lookup(IDS, (0x10de, 0x2786, 0x1043, 0x8902)).as_deref(), Some("TUF Gaming GeForce RTX 4070"));
        assert_eq!(lookup(IDS, (0x10de, 0x1c82, 0, 0)).as_deref(), Some("NVIDIA GeForce GTX 1050 Ti"));
        assert_eq!(lookup(IDS, (0x10de, 0x1eb8, 0, 0)).as_deref(), Some("NVIDIA Tesla T4"));
        let c = Card { vendor: Vendor::Nvidia, ids: (0x10de, 0x2786, 0, 0), ..Default::default() };
        assert_eq!(fallback_name(&c), "NVIDIA GPU (10de:2786)");
    }

    #[test]
    fn discovers_intel_and_nvidia_cards() {
        use std::os::unix::fs::symlink;
        let root = std::env::temp_dir().join(format!("drm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dev = |name: &str, driver: &str, vendor: &str| {
            let d = root.join("devices").join(name);
            std::fs::create_dir_all(root.join("drivers").join(driver)).unwrap();
            std::fs::create_dir_all(&d).unwrap();
            symlink(root.join("drivers").join(driver), d.join("driver")).unwrap();
            std::fs::write(d.join("vendor"), format!("{vendor}\n")).unwrap();
            d
        };
        let drm = root.join("drm");
        std::fs::create_dir_all(drm.join("card0")).unwrap();
        std::fs::create_dir_all(drm.join("card1")).unwrap();
        std::fs::create_dir_all(drm.join("card2")).unwrap();
        std::fs::create_dir_all(drm.join("card3")).unwrap();
        let i915 = dev("0000:00:02.0", "i915", "0x8086");
        let xe = dev("0000:04:00.0", "xe", "0x8086");
        let nv = dev("0000:01:00.0", "nouveau", "0x10de");
        symlink(&i915, drm.join("card0/device")).unwrap();
        symlink(&xe, drm.join("card1/device")).unwrap();
        symlink(&nv, drm.join("card2/device")).unwrap();
        let prop = dev("0000:02:00.0", "nvidia", "0x10de");
        std::fs::create_dir_all(prop.join("power")).unwrap();
        std::fs::write(prop.join("power/runtime_status"), "suspended\n").unwrap();
        symlink(&prop, drm.join("card3/device")).unwrap();
        std::fs::write(drm.join("card0/gt_act_freq_mhz"), "1300\n").unwrap();
        std::fs::create_dir_all(xe.join("tile0/gt0/freq0")).unwrap();
        std::fs::write(xe.join("tile0/gt0/freq0/act_freq"), "2050\n").unwrap();

        let cards = discover(&drm);
        // nouveau isn't supported; the proprietary driver is.
        assert_eq!(cards.len(), 3);
        assert_eq!((cards[0].slot.as_str(), cards[0].driver.as_str(), cards[0].vendor), ("0000:00:02.0", "i915", Vendor::Intel));
        assert_eq!((cards[1].slot.as_str(), cards[1].vendor), ("0000:02:00.0", Vendor::Nvidia));
        assert_eq!(cards[2].driver, "xe");
        assert_eq!(cards[0].read(true).clock_mhz, Some(1300.0));
        assert_eq!(cards[2].read(true).clock_mhz, Some(2050.0));
        // A sleeping NVIDIA card reads as idle without loading NVML.
        assert_eq!(cards[1].read(true), Reading { busy: Some(0.0), ..Default::default() });
        // No VRAM, no temperatures, and busy is the sampler's job.
        let r = cards[0].read(true);
        assert_eq!((r.vram_used, r.edge, r.busy), (None, None, None));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn amdgpu_ids_revisions() {
        let ids = include_str!("../../tests/fixtures/amdgpu.ids");
        assert_eq!(lookup_amdgpu_ids(ids, 0x7550, 0xc0).as_deref(), Some("AMD Radeon RX 9070 XT"));
        assert_eq!(lookup_amdgpu_ids(ids, 0x7550, 0xc3).as_deref(), Some("AMD Radeon RX 9070"));
        assert_eq!(lookup_amdgpu_ids(ids, 0x744c, 0xc8).as_deref(), Some("AMD Radeon RX 7900 XTX"));
        assert_eq!(lookup_amdgpu_ids(ids, 0x7550, 0xff), None);
    }

    #[test]
    fn dpm() {
        assert_eq!(dpm_sclk_mhz(include_str!("../../tests/fixtures/pp_dpm_sclk")), Some(1900.0));
        assert_eq!(dpm_sclk_mhz("0: 500Mhz\n"), None);
    }

    #[test]
    fn picks_most_vram() {
        let a = Card { slot: "a".into(), vram_total: Some(512 << 20), ..Default::default() };
        let b = Card { slot: "b".into(), vram_total: Some(20 << 30), ..Default::default() };
        let cards = [a, b];
        assert_eq!(pick(&cards, None).unwrap().slot, "b");
        assert_eq!(pick(&cards, Some("a")).unwrap().slot, "a");
        assert_eq!(pick(&cards, Some("zz")).unwrap().slot, "b");
        assert!(pick(&[], None).is_none());
    }
}
