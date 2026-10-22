//! GPU engine busy time from DRM fdinfo (`/proc/<pid>/fdinfo/<fd>`).
//!
//! i915 and xe have no `gpu_busy_percent`. Each open DRM file reports its
//! client's engine time instead (`drm-engine-<class>: N ns` on i915,
//! `drm-cycles-<class>` / `drm-total-cycles-<class>` on xe). Busy is the
//! per-client delta summed over clients and divided by the class's engine
//! count; the GPU's figure is its busiest class. Only processes this user
//! may inspect are seen, which on a desktop is the compositor and apps.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use common::sysfs::read_into;

pub const PROC: &str = "/proc";
/// Every process's fd table is re-read this often.
const RESCAN: Duration = Duration::from_secs(10);
/// New processes are looked for this often (listing `/proc` costs about
/// as much as reading every client).
const LIST: Duration = Duration::from_secs(2);
/// A second call inside this window returns the cached result.
const MIN_DT: Duration = Duration::from_millis(500);

/// One client's counters for one engine class.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Engine {
    /// Busy ns (i915) or busy cycles (xe).
    busy: u64,
    /// xe's GPU timestamp; `None` for ns counters.
    total: Option<u64>,
    capacity: u32,
}

/// One DRM file's fdinfo, as far as busy time goes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Client {
    pub pdev: String,
    pub id: u64,
    engines: Vec<(String, Engine)>,
}

pub fn parse(text: &str) -> Option<Client> {
    let mut c = Client::default();
    let mut has_id = false;
    let mut caps: Vec<(&str, u32)> = Vec::new();
    let mut totals: Vec<(&str, u64)> = Vec::new();
    for line in text.lines() {
        let Some((k, v)) = line.split_once(':') else { continue };
        let v = v.trim();
        let num = || v.split_whitespace().next().and_then(|n| n.parse::<u64>().ok());
        if k == "drm-pdev" {
            c.pdev = v.to_owned();
        } else if k == "drm-client-id" {
            c.id = num()?;
            has_id = true;
        } else if let Some(class) = k.strip_prefix("drm-engine-capacity-") {
            caps.push((class, num().unwrap_or(1) as u32));
        } else if let Some(class) = k.strip_prefix("drm-engine-") {
            c.engines.push((class.to_owned(), Engine { busy: num().unwrap_or(0), total: None, capacity: 1 }));
        } else if let Some(class) = k.strip_prefix("drm-total-cycles-") {
            totals.push((class, num().unwrap_or(0)));
        } else if let Some(class) = k.strip_prefix("drm-cycles-") {
            c.engines.push((class.to_owned(), Engine { busy: num().unwrap_or(0), total: None, capacity: 1 }));
        }
    }
    for (class, e) in &mut c.engines {
        if let Some(&(_, n)) = caps.iter().find(|(k, _)| k == class) {
            e.capacity = n.max(1);
        }
        if let Some(&(_, t)) = totals.iter().find(|(k, _)| k == class) {
            e.total = Some(t);
        }
    }
    has_id.then_some(c)
}

/// Busy fractions (0–100) for the GPU and two classes of interest.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Busy {
    pub overall: Option<f32>,
    pub render: Option<f32>,
    pub video: Option<f32>,
}

fn is_render(class: &str) -> bool {
    matches!(class, "render" | "rcs" | "gfx")
}

fn is_video(class: &str) -> bool {
    matches!(class, "video" | "vcs")
}

/// Busy per class between two samples of the same clients, `dt` apart.
/// Clients in only one sample (opened or closed meanwhile) are skipped.
pub fn busy(prev: &HashMap<u64, Client>, now: &HashMap<u64, Client>, dt: Duration) -> Busy {
    let dt_ns = dt.as_nanos() as f64;
    let mut per_class: HashMap<&str, (f64, u32)> = HashMap::new();
    for (id, c) in now {
        for (class, e) in &c.engines {
            let slot = per_class.entry(class.as_str()).or_insert((0.0, e.capacity));
            slot.1 = slot.1.max(e.capacity);
            let Some(p) = prev.get(id).and_then(|p| p.engines.iter().find(|(k, _)| k == class)).map(|(_, e)| e) else { continue };
            let Some(d) = e.busy.checked_sub(p.busy) else { continue };
            let frac = match (e.total, p.total) {
                (Some(t), Some(pt)) if t > pt => d as f64 / (t - pt) as f64,
                (None, None) if dt_ns > 0.0 => d as f64 / dt_ns,
                _ => continue,
            };
            slot.0 += frac;
        }
    }
    if per_class.is_empty() || dt_ns <= 0.0 {
        // No client on this GPU: nothing is running on it.
        return Busy { overall: Some(0.0), render: Some(0.0), video: Some(0.0) };
    }
    let pct = |(sum, cap): (f64, u32)| ((sum / cap.max(1) as f64).clamp(0.0, 1.0) * 100.0) as f32;
    let class = |f: fn(&str) -> bool| Some(per_class.iter().filter(|(k, _)| f(k)).map(|(_, v)| pct(*v)).fold(0.0, f32::max));
    Busy { overall: per_class.values().map(|v| pct(*v)).reduce(f32::max), render: class(is_render), video: class(is_video) }
}

/// Tracks which fds are DRM files and samples their busy time.
#[derive(Debug)]
pub struct Clients {
    root: PathBuf,
    /// pid → fds that point at `/dev/dri/*`, pruned to one fd per client of
    /// the GPU on show after the first read.
    procs: HashMap<u32, Vec<u32>>,
    last_scan: Option<Instant>,
    last_list: Option<Instant>,
    prev: Option<(Instant, HashMap<u64, Client>)>,
    latest: Busy,
    buf: String,
}

impl Default for Clients {
    fn default() -> Self {
        Self::new(Path::new(PROC))
    }
}

impl Clients {
    pub fn new(root: &Path) -> Self {
        Self { root: root.to_path_buf(), procs: HashMap::new(), last_scan: None, last_list: None, prev: None, latest: Busy::default(), buf: String::new() }
    }

    /// Forget the baseline and the pruned fds, e.g. when the GPU on show
    /// changes.
    pub fn reset(&mut self) {
        self.prev = None;
        self.last_scan = None;
        self.latest = Busy::default();
    }

    fn drm_fds(&self, pid: u32) -> Vec<u32> {
        let dir = self.root.join(pid.to_string()).join("fd");
        let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
        entries
            .flatten()
            .filter(|e| std::fs::read_link(e.path()).is_ok_and(|t| t.starts_with("/dev/dri/")))
            .filter_map(|e| e.file_name().to_str()?.parse().ok())
            .collect()
    }

    fn scan(&mut self, now: Instant) {
        let full = self.last_scan.is_none_or(|t| now.duration_since(t) >= RESCAN);
        if !full && self.last_list.is_some_and(|t| now.duration_since(t) < LIST) {
            return;
        }
        self.last_list = Some(now);
        let Ok(entries) = std::fs::read_dir(&self.root) else { return };
        let pids: HashSet<u32> = entries.flatten().filter_map(|e| e.file_name().to_str()?.parse().ok()).collect();
        self.procs.retain(|pid, _| pids.contains(pid));
        for pid in pids {
            if full || !self.procs.contains_key(&pid) {
                let fds = self.drm_fds(pid);
                self.procs.insert(pid, fds);
            }
        }
        if full {
            self.last_scan = Some(now);
        }
    }

    /// Busy for the GPU at PCI slot `pdev`. The first call only sets the
    /// baseline (busy `None`).
    pub fn sample(&mut self, now: Instant, pdev: &str) -> Busy {
        if self.prev.as_ref().is_some_and(|(t, _)| now.duration_since(*t) < MIN_DT) {
            return self.latest;
        }
        self.scan(now);
        let mut clients: HashMap<u64, Client> = HashMap::new();
        let mut closed = false;
        for (&pid, fds) in &mut self.procs {
            fds.retain(|&fd| {
                if read_into(self.root.join(format!("{pid}/fdinfo/{fd}")), &mut self.buf).is_err() {
                    closed = true;
                    return false;
                }
                // Several fds (dup, fork) can share one client: read it
                // through one of them. Other GPUs' clients are dropped too.
                match parse(&self.buf).filter(|c| c.pdev == pdev) {
                    Some(c) if !clients.contains_key(&c.id) => {
                        clients.insert(c.id, c);
                        true
                    }
                    _ => false,
                }
            });
        }
        if closed {
            // The fd kept for a client may have had open duplicates: find
            // them on the next tick.
            self.last_scan = None;
        }
        self.latest = match &self.prev {
            Some((t, prev)) => busy(prev, &clients, now.duration_since(*t)),
            None => Busy::default(),
        };
        self.prev = Some((now, clients));
        self.latest
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const I915: &str = include_str!("../../tests/fixtures/fdinfo-i915");
    const XE: &str = include_str!("../../tests/fixtures/fdinfo-xe");

    #[test]
    fn parses_i915() {
        let c = parse(I915).unwrap();
        assert_eq!(c.pdev, "0000:00:02.0");
        assert_eq!(c.id, 7);
        let video = c.engines.iter().find(|(k, _)| k == "video").unwrap().1;
        assert_eq!(video, Engine { busy: 2_000_000_000, total: None, capacity: 2 });
        assert_eq!(c.engines.len(), 5);
    }

    #[test]
    fn parses_xe() {
        let c = parse(XE).unwrap();
        assert_eq!(c.id, 12);
        let rcs = c.engines.iter().find(|(k, _)| k == "rcs").unwrap().1;
        assert_eq!(rcs, Engine { busy: 1_000_000, total: Some(10_000_000), capacity: 1 });
        let vcs = c.engines.iter().find(|(k, _)| k == "vcs").unwrap().1;
        assert_eq!(vcs.capacity, 2);
        assert!(parse("pos:\t0\nflags:\t02\n").is_none());
    }

    #[test]
    fn i915_busy() {
        let a = parse(I915).unwrap();
        // Render +250 ms, video +500 ms over two engines, in one second.
        let b = parse(&I915.replace("render:\t1000000000", "render:\t1250000000").replace("video:\t2000000000", "video:\t2500000000")).unwrap();
        let prev = HashMap::from([(a.id, a)]);
        let now = HashMap::from([(b.id, b)]);
        let busy = busy(&prev, &now, Duration::from_secs(1));
        assert_eq!(busy.render, Some(25.0));
        assert_eq!(busy.video, Some(25.0));
        assert_eq!(busy.overall, Some(25.0));
    }

    #[test]
    fn xe_busy() {
        let a = parse(XE).unwrap();
        // rcs +6M of +10M cycles; the GPU timestamp moves for every class.
        let b = parse(&XE.replace("drm-cycles-rcs:\t1000000", "drm-cycles-rcs:\t7000000").replace("10000000", "20000000")).unwrap();
        let busy = busy(&HashMap::from([(a.id, a)]), &HashMap::from([(b.id, b)]), Duration::from_secs(1));
        assert_eq!(busy.render, Some(60.0));
        assert_eq!(busy.overall, Some(60.0));
        assert_eq!(busy.video, Some(0.0));
    }

    #[test]
    fn new_and_closed_clients_are_skipped() {
        let a = parse(I915).unwrap();
        let mut b = parse(&I915.replace("drm-engine-render:\t1000000000", "drm-engine-render:\t9000000000")).unwrap();
        b.id = 99;
        let busy = busy(&HashMap::from([(a.id, a)]), &HashMap::from([(b.id, b)]), Duration::from_secs(1));
        assert_eq!(busy.overall, Some(0.0));
        let none = super::busy(&HashMap::new(), &HashMap::new(), Duration::from_secs(1));
        assert_eq!(none.overall, Some(0.0));
    }

    #[test]
    fn scans_a_proc_tree() {
        use std::os::unix::fs::symlink;
        let root = std::env::temp_dir().join(format!("fdinfo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for pid in ["100", "200"] {
            std::fs::create_dir_all(root.join(pid).join("fd")).unwrap();
            std::fs::create_dir_all(root.join(pid).join("fdinfo")).unwrap();
        }
        symlink("/dev/dri/renderD128", root.join("100/fd/5")).unwrap();
        symlink("/dev/null", root.join("100/fd/0")).unwrap();
        // pid 200 shares the client through a dup'd fd.
        symlink("/dev/dri/card0", root.join("200/fd/9")).unwrap();
        std::fs::write(root.join("100/fdinfo/5"), I915).unwrap();
        std::fs::write(root.join("200/fdinfo/9"), I915).unwrap();

        let mut c = Clients::new(&root);
        let t0 = Instant::now();
        assert_eq!(c.sample(t0, "0000:00:02.0"), Busy::default());
        // /dev/null is never a client, and the shared client keeps one fd.
        assert_eq!(c.procs.values().map(Vec::len).sum::<usize>(), 1);
        let more = I915.replace("drm-engine-render:\t1000000000", "drm-engine-render:\t1500000000");
        std::fs::write(root.join("100/fdinfo/5"), &more).unwrap();
        std::fs::write(root.join("200/fdinfo/9"), &more).unwrap();
        let b = c.sample(t0 + Duration::from_secs(1), "0000:00:02.0");
        // Counted once, not twice.
        assert_eq!(b.render, Some(50.0));
        // Inside the minimum interval: cached.
        assert_eq!(c.sample(t0 + Duration::from_millis(1100), "0000:00:02.0"), b);
        // Another GPU's clients don't count.
        c.reset();
        c.sample(t0 + Duration::from_secs(2), "0000:03:00.0");
        assert_eq!(c.sample(t0 + Duration::from_secs(3), "0000:03:00.0").overall, Some(0.0));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
