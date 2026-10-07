//! The 1 Hz sampler: counters for every interface, 60-sample histories,
//! session totals, and the default route.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use common::ring::Ring;
use common::sysfs::{Counter, read_u64};

use super::{Iface, enumerate, has_carrier, is_up, route};

const SYS_NET: &str = "/sys/class/net";
const REDISCOVER: Duration = Duration::from_secs(5);
const MIN_DT: f64 = 0.5;

/// One interface's history.
#[derive(Debug, Default)]
pub struct Series {
    pub down: Ring,
    pub up: Ring,
    rx: Counter,
    tx: Counter,
    rx_path: PathBuf,
    tx_path: PathBuf,
    /// Consecutive failed counter reads.
    pub fails: u8,
}

impl Series {
    fn new(dir: &Path) -> Self {
        let stats = dir.join("statistics");
        Self { rx_path: stats.join("rx_bytes"), tx_path: stats.join("tx_bytes"), ..Default::default() }
    }

    pub fn totals(&self) -> (u64, u64) {
        (self.rx.total, self.tx.total)
    }

    /// Last download and upload rates.
    pub fn last(&self) -> (Option<f32>, Option<f32>) {
        (self.down.last(), self.up.last())
    }
}

pub struct Sampler {
    root: PathBuf,
    pub ifaces: Vec<Iface>,
    pub series: HashMap<String, Series>,
    /// Interface carrying the default route, if any.
    pub default_route: Option<String>,
    last_tick: Option<Instant>,
    last_discover: Option<Instant>,
    buf: String,
}

impl Default for Sampler {
    fn default() -> Self {
        Self::new(SYS_NET)
    }
}

impl Sampler {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into(), ifaces: Vec::new(), series: HashMap::new(), default_route: None, last_tick: None, last_discover: None, buf: String::new() }
    }

    /// Re-read the interface list and the default route.
    pub fn discover(&mut self) {
        self.ifaces = enumerate(&self.root);
        for i in &self.ifaces {
            self.series.entry(i.name.clone()).or_insert_with(|| Series::new(&i.dir));
        }
        self.series.retain(|name, _| self.ifaces.iter().any(|i| &i.name == name));
        self.default_route = self.read_default_route();
        self.last_discover = Some(Instant::now());
    }

    fn read_default_route(&mut self) -> Option<String> {
        let found = common::sysfs::read_into("/proc/net/route", &mut self.buf).ok().and_then(|()| route::ipv4_default(&self.buf));
        found.or_else(|| common::sysfs::read_into("/proc/net/ipv6_route", &mut self.buf).ok().and_then(|()| route::ipv6_default(&self.buf)))
    }

    /// Take one sample of every interface. `watch` names the interface on
    /// show, whose up/carrier state is refreshed every tick so unplugging
    /// shows within a second.
    pub fn tick(&mut self, now: Instant, watch: Option<&str>) {
        if self.last_discover.is_none_or(|t| now.duration_since(t) >= REDISCOVER) {
            self.discover();
        }
        let dt = self.last_tick.map_or(0.0, |t| now.duration_since(t).as_secs_f64());
        if self.last_tick.is_some() && dt < MIN_DT {
            // Two ticks back to back (start-up, popup open): too short a
            // window for a steady rate.
            return;
        }
        self.last_tick = Some(now);
        let mut failed = false;
        for s in self.series.values_mut() {
            let rx = read_u64(&s.rx_path);
            let tx = read_u64(&s.tx_path);
            if rx.is_none() || tx.is_none() {
                s.fails = s.fails.saturating_add(1);
                failed = true;
            } else {
                s.fails = 0;
            }
            if dt > 0.0 {
                s.down.push(s.rx.update(rx, dt).map(|v| v as f32));
                s.up.push(s.tx.update(tx, dt).map(|v| v as f32));
            } else {
                // First reading: baseline only.
                s.rx.update(rx, 0.0);
                s.tx.update(tx, 0.0);
            }
        }
        if let Some(i) = watch.and_then(|w| self.ifaces.iter_mut().find(|i| i.name == w)) {
            i.up = is_up(&i.dir);
            i.carrier = has_carrier(&i.dir);
        }
        if failed {
            // Something went away: re-enumerate on the next tick.
            self.last_discover = None;
        }
    }

    pub fn iface(&self, name: &str) -> Option<&Iface> {
        self.ifaces.iter().find(|i| i.name == name)
    }
}
