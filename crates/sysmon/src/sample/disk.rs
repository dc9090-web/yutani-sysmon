//! Physical drives under `/sys/block`.

use std::path::{Path, PathBuf};

use common::sysfs::{read_string, read_u64};

use super::hwmon;

pub const BLOCK: &str = "/sys/block";
const EXCLUDE: [&str; 5] = ["loop", "ram", "zram", "dm-", "md"];

#[derive(Debug, Clone)]
pub struct Drive {
    pub name: String,
    pub model: String,
    pub size: u64,
    pub stat: PathBuf,
    /// NVMe `temp1_input` and its `temp1_max`.
    pub temp: Option<PathBuf>,
    pub temp_max: Option<f32>,
}

pub fn discover(root: &Path) -> Vec<Drive> {
    let Ok(entries) = std::fs::read_dir(root) else { return Vec::new() };
    let mut v: Vec<Drive> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let dir = e.path();
            if EXCLUDE.iter().any(|x| name.starts_with(x)) || !dir.join("device").exists() {
                return None;
            }
            let dev = dir.join("device");
            let hw = hwmon::first_in(&dev).or_else(|| {
                // /sys/class/nvme/nvmeN/hwmon*
                let ctrl = name.split('n').take(2).collect::<Vec<_>>().join("n");
                hwmon::first_in(&Path::new("/sys/class/nvme").join(ctrl))
            });
            let temp = hw.as_ref().map(|h| h.join("temp1_input")).filter(|p| p.exists());
            Some(Drive {
                model: read_string(dev.join("model")).unwrap_or_else(|| name.clone()),
                size: read_u64(dir.join("size")).unwrap_or(0) * 512,
                stat: dir.join("stat"),
                temp_max: temp.as_ref().and_then(|t| hwmon::sibling(t, "max")).and_then(read_u64).map(|m| m as f32 / 1000.0),
                temp,
                name,
            })
        })
        .collect();
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v
}

/// `(read, written)` bytes: fields 3 and 7 of `stat`, × 512.
pub fn parse_stat(text: &str) -> Option<(u64, u64)> {
    let f: Vec<&str> = text.split_whitespace().collect();
    Some((f.get(2)?.parse::<u64>().ok()? * 512, f.get(6)?.parse::<u64>().ok()? * 512))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_stat() {
        let (r, w) = parse_stat(include_str!("../../tests/fixtures/block-stat")).unwrap();
        assert!(r > 0 && w > 0);
        assert_eq!(parse_stat("1 2 3 4 5 6 7 8"), Some((3 * 512, 7 * 512)));
        assert_eq!(parse_stat("1 2"), None);
    }
}
