//! The 1 Hz sampler: every metric's history and latest reading.
//!
//! Per tick it reads `/proc/stat`, `/proc/meminfo`, each drive's `stat`,
//! the GPU busy and temperature files, Tctl and RAPL. Clocks, VRAM, GPU
//! power and NVMe temps are only read while the popup shows them.

pub mod cpu;
pub mod disk;
pub mod gpu;
pub mod hwmon;
pub mod mem;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use common::ring::Ring;
use common::sysfs::{Counter, read_into, read_u64};

const REDISCOVER: Duration = Duration::from_secs(30);
const MIN_DT: f64 = 0.5;

/// Warning state with hysteresis: on at the threshold, off only once the
/// value is 3 below it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Hot(pub bool);

impl Hot {
    pub const MARGIN: f32 = 3.0;

    pub fn update(&mut self, v: Option<f32>, threshold: f32) -> bool {
        self.0 = match v {
            Some(v) if v >= threshold => true,
            Some(v) if v < threshold - Self::MARGIN => false,
            Some(_) => self.0,
            None => false,
        };
        self.0
    }
}

#[derive(Debug, Default)]
pub struct DiskSeries {
    pub read: Ring,
    pub write: Ring,
    rc: Counter,
    wc: Counter,
}

impl DiskSeries {
    pub fn totals(&self) -> (u64, u64) {
        (self.rc.total, self.wc.total)
    }
}

/// Which drive and GPU the user picked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Choice {
    pub disk: Option<String>,
    pub gpu: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Latest {
    pub cpu_pct: Option<f32>,
    pub cpu_mhz: Option<f32>,
    pub tctl: Option<f32>,
    pub pkg_w: Option<f32>,
    pub gpu: gpu::Reading,
    pub mem: Option<mem::Mem>,
    pub nvme_temp: Option<f32>,
    pub nvme_max: Option<f32>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Warnings {
    pub cpu: Hot,
    pub gpu_junction: Hot,
    pub gpu_edge: Hot,
    pub nvme: Hot,
    pub ram: Hot,
    pub vram: Hot,
}

impl Warnings {
    pub fn gpu(&self) -> bool {
        self.gpu_junction.0 || self.gpu_edge.0
    }
}

#[derive(Default)]
pub struct Sampler {
    pub info: cpu::Info,
    pub cards: Vec<gpu::Card>,
    /// Slot of the card on show, and its looked-up name.
    pub gpu_name: Option<(String, String)>,
    pub drives: Vec<disk::Drive>,
    /// The chosen drive is missing; showing All.
    pub disk_missing: bool,

    pub cpu: Ring,
    pub gpu: Ring,
    pub mem: Ring,
    /// Summed over the shown drive(s).
    pub disk: DiskSeries,
    pub latest: Latest,
    pub warn: Warnings,

    choice: Choice,
    freq: Vec<PathBuf>,
    tctl: Option<PathBuf>,
    rapl: cpu::Rapl,
    prev_stat: Option<(u64, u64)>,
    last_tick: Option<Instant>,
    last_discover: Option<Instant>,
    buf: String,
}

impl Sampler {
    pub fn new(choice: Choice) -> Self {
        let mut s = Self { choice, ..Default::default() };
        if read_into("/proc/cpuinfo", &mut s.buf).is_ok() {
            s.info = cpu::parse_cpuinfo(&s.buf);
        }
        s.buf = String::new();
        s.discover();
        s
    }

    /// Resolve devices and sensor paths. At start, every 30 s, and when the
    /// popup opens.
    pub fn discover(&mut self) {
        let hw = Path::new(hwmon::HWMON);
        self.freq = cpu::freq_paths();
        self.tctl = cpu::tctl_path(hw);
        self.rapl = cpu::Rapl::new(Path::new(cpu::RAPL));
        self.cards = gpu::discover(Path::new(gpu::DRM));
        let drives = disk::discover(Path::new(disk::BLOCK));
        if drives.iter().map(|d| &d.name).ne(self.drives.iter().map(|d| &d.name)) {
            // A different set of drives: the sum restarts from a new baseline.
            self.disk = DiskSeries { read: std::mem::take(&mut self.disk.read), write: std::mem::take(&mut self.disk.write), ..Default::default() };
        }
        self.drives = drives;
        self.resolve_names();
        self.last_discover = Some(Instant::now());
    }

    fn resolve_names(&mut self) {
        let card = gpu::pick(&self.cards, self.choice.gpu.as_deref());
        match card {
            Some(c) if self.gpu_name.as_ref().is_none_or(|(slot, _)| slot != &c.slot) => {
                self.gpu_name = Some((c.slot.clone(), gpu::name(c)));
            }
            None => self.gpu_name = None,
            _ => {}
        }
        self.disk_missing = self.choice.disk.as_ref().is_some_and(|d| !self.drives.iter().any(|x| &x.name == d));
    }

    pub fn set_choice(&mut self, choice: Choice) {
        if choice != self.choice {
            let disk_changed = choice.disk != self.choice.disk;
            self.choice = choice;
            if disk_changed {
                self.disk = DiskSeries::default();
            }
            self.resolve_names();
        }
    }

    pub fn card(&self) -> Option<&gpu::Card> {
        gpu::pick(&self.cards, self.choice.gpu.as_deref())
    }

    /// One sample. `full` adds the popup-only readings.
    pub fn tick(&mut self, now: Instant, full: bool) {
        if self.last_discover.is_none_or(|t| now.duration_since(t) >= REDISCOVER) {
            self.discover();
        }
        let dt = self.last_tick.map_or(0.0, |t| now.duration_since(t).as_secs_f64());
        if self.last_tick.is_some() && dt < MIN_DT {
            return;
        }
        self.last_tick = Some(now);
        let first = dt == 0.0;

        // CPU
        let stat = read_into("/proc/stat", &mut self.buf).ok().and_then(|()| cpu::parse_stat(&self.buf));
        let pct = match (self.prev_stat, stat) {
            (Some(a), Some(b)) => cpu::usage(a, b),
            _ => None,
        };
        self.prev_stat = stat;
        self.latest.cpu_pct = pct;
        self.latest.tctl = self.tctl.as_ref().and_then(read_u64).map(|m| m as f32 / 1000.0);
        self.latest.pkg_w = self.rapl.sample(dt);

        // GPU (with the popup open, `read_extras` reads it in full below)
        if !full {
            self.latest.gpu = self.card().map(|c| c.read(false)).unwrap_or_default();
        }

        // Memory
        self.latest.mem = read_into("/proc/meminfo", &mut self.buf).ok().and_then(|()| mem::parse(&self.buf));

        // Disk
        let mut sum: Option<(u64, u64)> = Some((0, 0));
        let only = self.choice.disk.clone().filter(|_| !self.disk_missing);
        for d in self.drives.iter().filter(|d| only.as_deref().is_none_or(|n| d.name == n)) {
            let rw = read_into(&d.stat, &mut self.buf).ok().and_then(|()| disk::parse_stat(&self.buf));
            sum = match (sum, rw) {
                (Some((r, w)), Some((dr, dw))) => Some((r + dr, w + dw)),
                _ => None,
            };
        }
        let (r, w) = (sum.map(|s| s.0), sum.map(|s| s.1));
        let rr = self.disk.rc.update(r, dt).map(|v| v as f32);
        let wr = self.disk.wc.update(w, dt).map(|v| v as f32);
        if full {
            self.read_extras();
        } else {
            self.latest.cpu_mhz = None;
            self.latest.nvme_temp = None;
        }

        if !first {
            self.cpu.push(pct);
            self.gpu.push(self.latest.gpu.busy);
            self.mem.push(self.latest.mem.and_then(|m| m.ram_pct()));
            self.disk.read.push(rr);
            self.disk.write.push(wr);
        }
        self.update_warnings();
    }

    /// The popup-only readings: average clock, the GPU in full (VRAM,
    /// clock, power) and NVMe temperature. Called every tick while the
    /// popup is open, and once as it opens so it never shows stale dashes.
    pub fn read_extras(&mut self) {
        self.latest.cpu_mhz = cpu::mean_mhz(&self.freq);
        self.latest.gpu = self.card().map(|c| c.read(true)).unwrap_or_default();
        let only = self.choice.disk.as_deref().filter(|_| !self.disk_missing);
        let hottest = self
            .drives
            .iter()
            .filter(|d| only.is_none_or(|n| d.name == n))
            .filter_map(|d| d.temp.as_ref().and_then(read_u64).map(|m| (m as f32 / 1000.0, d.temp_max)))
            .max_by(|a, b| a.0.total_cmp(&b.0));
        self.latest.nvme_temp = hottest.map(|h| h.0);
        self.latest.nvme_max = hottest.and_then(|h| h.1);
        self.update_warnings();
    }

    fn update_warnings(&mut self) {
        let l = &self.latest;
        self.warn.cpu.update(l.tctl, 85.0);
        let jt = self.card().and_then(|c| c.junction_crit).map_or(95.0, |crit| crit as f32 / 1000.0 - 10.0);
        let (j, e) = (l.gpu.junction, l.gpu.edge);
        self.warn.gpu_junction.update(j, jt);
        self.warn.gpu_edge.update(e, 85.0);
        // NVMe is only read with the popup open; keep its state otherwise.
        if l.nvme_temp.is_some() {
            let max = l.nvme_max.unwrap_or(70.0);
            self.warn.nvme.update(l.nvme_temp, max);
        }
        self.warn.ram.update(l.mem.and_then(|m| m.ram_pct()), 90.0);
        let vram = l.gpu.vram_used.zip(self.card().and_then(|c| c.vram_total)).map(|(u, t)| (u as f64 / t as f64 * 100.0) as f32);
        if vram.is_some() {
            self.warn.vram.update(vram, 95.0);
        }
    }

    pub fn rapl_denied(&self) -> bool {
        self.rapl.denied
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hysteresis() {
        let mut h = Hot::default();
        assert!(!h.update(Some(84.0), 85.0));
        assert!(h.update(Some(85.0), 85.0));
        assert!(h.update(Some(83.0), 85.0));
        assert!(h.update(Some(82.0), 85.0));
        assert!(!h.update(Some(81.9), 85.0));
        assert!(!h.update(Some(84.0), 85.0));
        assert!(!h.update(None, 85.0));
    }
}
