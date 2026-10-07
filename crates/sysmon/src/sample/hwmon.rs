//! hwmon lookups: a chip by `name`, a sensor by its `*_label`.

use std::path::{Path, PathBuf};

use common::sysfs::read_string;

pub const HWMON: &str = "/sys/class/hwmon";

/// Every hwmon directory under `root` whose `name` is `name`.
pub fn by_name(root: &Path, name: &str) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(root) else { return Vec::new() };
    let mut out: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| read_string(p.join("name")).as_deref() == Some(name)).collect();
    out.sort();
    out
}

/// The first hwmon directory inside `dir` (e.g. `device/hwmon/hwmon2`, or
/// `device/hwmon2` for NVMe).
pub fn first_in(dir: &Path) -> Option<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else { return None };
    let mut v: Vec<PathBuf> = entries.flatten().filter(|e| e.file_name().to_string_lossy().starts_with("hwmon")).map(|e| e.path()).collect();
    v.sort();
    v.into_iter().next()
}

/// `<kind>N_input` whose `<kind>N_label` is `label`, e.g. ("temp", "Tctl").
pub fn input_by_label(hwmon: &Path, kind: &str, label: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(hwmon).ok()?;
    for e in entries.flatten() {
        let name = e.file_name();
        let name = name.to_string_lossy();
        let Some(n) = name.strip_prefix(kind).and_then(|r| r.strip_suffix("_label")) else { continue };
        if read_string(e.path()).as_deref() == Some(label) {
            let input = hwmon.join(format!("{kind}{n}_input"));
            if input.exists() {
                return Some(input);
            }
        }
    }
    None
}

/// The sibling attribute of an input, e.g. `temp2_input` → `temp2_crit`.
pub fn sibling(input: &Path, suffix: &str) -> Option<PathBuf> {
    let name = input.file_name()?.to_str()?;
    let stem = name.strip_suffix("_input")?;
    let p = input.with_file_name(format!("{stem}_{suffix}"));
    p.exists().then_some(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fx() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hwmon")
    }

    #[test]
    fn finds_k10temp_tctl() {
        let k = by_name(&fx(), "k10temp");
        assert_eq!(k.len(), 1);
        let tctl = input_by_label(&k[0], "temp", "Tctl").unwrap();
        assert!(tctl.ends_with("hwmon1/temp1_input"));
        assert_eq!(input_by_label(&k[0], "temp", "Package id 0"), None);
    }

    #[test]
    fn gpu_labels_and_siblings() {
        let g = by_name(&fx(), "amdgpu").remove(0);
        let j = input_by_label(&g, "temp", "junction").unwrap();
        assert!(sibling(&j, "crit").unwrap().ends_with("temp2_crit"));
        assert!(input_by_label(&g, "freq", "sclk").is_some());
        // APU without a junction sensor.
        let apu = fx().parent().unwrap().join("apu-hwmon");
        assert!(input_by_label(&apu, "temp", "junction").is_none());
        assert!(input_by_label(&apu, "temp", "edge").is_some());
    }
}
