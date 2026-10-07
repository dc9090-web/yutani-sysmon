//! CPU: `/proc/stat` usage, `/proc/cpuinfo` identity, cpufreq clock,
//! k10temp/coretemp Tctl, and RAPL package power.

use std::collections::HashSet;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use common::sysfs::{read_u64, with_small};

use super::hwmon;

/// `(busy, total)` jiffies from the aggregate `cpu` line.
pub fn parse_stat(text: &str) -> Option<(u64, u64)> {
    let line = text.lines().next()?;
    let mut it = line.split_whitespace();
    if it.next()? != "cpu" {
        return None;
    }
    // user nice system idle iowait irq softirq steal (guest is in user).
    let v: Vec<u64> = it.take(8).map(|f| f.parse().unwrap_or(0)).collect();
    if v.len() < 5 {
        return None;
    }
    let total: u64 = v.iter().sum();
    Some((total - v[3] - v[4], total))
}

/// Usage % between two `(busy, total)` readings.
pub fn usage(prev: (u64, u64), now: (u64, u64)) -> Option<f32> {
    let dt = now.1.checked_sub(prev.1)?;
    let db = now.0.checked_sub(prev.0)?;
    (dt > 0).then(|| (db as f64 / dt as f64 * 100.0) as f32)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Info {
    pub model: String,
    pub cores: usize,
    pub threads: usize,
}

/// Trim, collapse spaces, drop a trailing "N-Core Processor".
pub fn clean_model(raw: &str) -> String {
    let mut words: Vec<&str> = raw.split_whitespace().collect();
    if words.len() >= 2 && words[words.len() - 1] == "Processor" && words[words.len() - 2].ends_with("-Core") {
        words.truncate(words.len() - 2);
    }
    words.join(" ")
}

pub fn parse_cpuinfo(text: &str) -> Info {
    let mut model = None;
    let mut threads = 0;
    let mut cores = HashSet::new();
    let (mut phys, mut core) = (None::<String>, None::<String>);
    for line in text.lines().chain(std::iter::once("")) {
        if line.trim().is_empty() {
            if let (Some(p), Some(c)) = (phys.take(), core.take()) {
                cores.insert((p, c));
            }
            continue;
        }
        let Some((k, v)) = line.split_once(':') else { continue };
        let (k, v) = (k.trim(), v.trim());
        match k {
            "processor" => threads += 1,
            "model name" if model.is_none() => model = Some(clean_model(v)),
            "physical id" => phys = Some(v.to_owned()),
            "core id" => core = Some(v.to_owned()),
            _ => {}
        }
    }
    Info { model: model.unwrap_or_default(), cores: if cores.is_empty() { threads } else { cores.len() }, threads }
}

/// `scaling_cur_freq` of every online CPU.
pub fn freq_paths() -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir("/sys/devices/system/cpu") else { return Vec::new() };
    let mut v: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| {
            let n = e.file_name();
            let n = n.to_string_lossy();
            n.strip_prefix("cpu").is_some_and(|r| !r.is_empty() && r.bytes().all(|b| b.is_ascii_digit()))
        })
        .filter(|e| read_u64(e.path().join("online")).is_none_or(|o| o == 1))
        .map(|e| e.path().join("cpufreq/scaling_cur_freq"))
        .filter(|p| p.exists())
        .collect();
    v.sort();
    v
}

/// Mean clock in MHz.
pub fn mean_mhz(paths: &[PathBuf]) -> Option<f32> {
    let khz: Vec<u64> = paths.iter().filter_map(read_u64).collect();
    (!khz.is_empty()).then(|| (khz.iter().sum::<u64>() as f64 / khz.len() as f64 / 1000.0) as f32)
}

/// Tctl from k10temp, else coretemp's "Package id 0".
pub fn tctl_path(hwmon_root: &Path) -> Option<PathBuf> {
    hwmon::by_name(hwmon_root, "k10temp")
        .iter()
        .find_map(|h| hwmon::input_by_label(h, "temp", "Tctl"))
        .or_else(|| hwmon::by_name(hwmon_root, "coretemp").iter().find_map(|h| hwmon::input_by_label(h, "temp", "Package id 0")))
}

pub const RAPL: &str = "/sys/class/powercap/intel-rapl:0";

/// RAPL energy counter. Root-only by default (CVE-2020-8694); without
/// access it reports `denied` and is never retried until re-discovery.
#[derive(Debug, Default)]
pub struct Rapl {
    energy: PathBuf,
    max_range: Option<u64>,
    last: Option<u64>,
    pub denied: bool,
    missing: bool,
}

impl Rapl {
    pub fn new(dir: &Path) -> Self {
        let energy = dir.join("energy_uj");
        let missing = !energy.exists();
        Self { max_range: read_u64(dir.join("max_energy_range_uj")), energy, missing, ..Default::default() }
    }

    /// Watts over `dt` seconds.
    pub fn sample(&mut self, dt: f64) -> Option<f32> {
        if self.denied || self.missing {
            return None;
        }
        let now = match with_small(&self.energy, |s| s.parse::<u64>().ok()) {
            Ok(v) => v,
            Err(e) if e.kind() == ErrorKind::PermissionDenied => {
                tracing::warn!("{}: permission denied; package power needs the optional udev rule", self.energy.display());
                self.denied = true;
                return None;
            }
            Err(_) => None,
        };
        let prev = self.last;
        self.last = now;
        let (prev, now) = (prev?, now?);
        let d = if now >= prev { now - prev } else { self.max_range? - prev + now };
        (dt > 0.0).then(|| (d as f64 / dt / 1e6) as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stat_usage() {
        let a = parse_stat(include_str!("../../tests/fixtures/stat-a")).unwrap();
        let b = parse_stat(include_str!("../../tests/fixtures/stat-b")).unwrap();
        let u = usage(a, b).unwrap();
        assert!((0.0..=100.0).contains(&u));
        let x = parse_stat("cpu  100 0 100 700 100 0 0 0 0 0\n").unwrap();
        assert_eq!(x, (200, 1000));
        let y = parse_stat("cpu  200 0 200 1300 300 0 0 0 0 0\n").unwrap();
        assert_eq!(usage(x, y), Some(20.0));
        assert_eq!(parse_stat("intr 1 2 3"), None);
    }

    #[test]
    fn cpuinfo() {
        let i = parse_cpuinfo(include_str!("../../tests/fixtures/cpuinfo-ryzen"));
        assert_eq!(i.model, "AMD Ryzen 9 3950X");
        assert_eq!((i.cores, i.threads), (16, 32));
    }

    #[test]
    fn models() {
        assert_eq!(clean_model("  AMD Ryzen 9 7950X 16-Core Processor  "), "AMD Ryzen 9 7950X");
        assert_eq!(clean_model("Intel(R) Core(TM)  i7-8700K CPU @ 3.70GHz"), "Intel(R) Core(TM) i7-8700K CPU @ 3.70GHz");
    }

    #[test]
    fn rapl_wraps_and_denies() {
        let dir = std::env::temp_dir().join(format!("rapl-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("max_energy_range_uj"), "1000000\n").unwrap();
        std::fs::write(dir.join("energy_uj"), "900000\n").unwrap();
        let mut r = Rapl::new(&dir);
        assert_eq!(r.sample(1.0), None);
        std::fs::write(dir.join("energy_uj"), "100000\n").unwrap();
        assert_eq!(r.sample(1.0), Some(0.2));
        std::fs::remove_dir_all(&dir).unwrap();
        // Missing counter: always None, never an error.
        let mut m = Rapl::new(Path::new("/nonexistent"));
        assert_eq!(m.sample(1.0), None);
    }
}
