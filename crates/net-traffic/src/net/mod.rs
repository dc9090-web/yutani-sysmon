//! Network interfaces from `/sys/class/net`.

pub mod route;
pub mod sampler;

use std::path::{Path, PathBuf};

use common::sysfs::{read_i64, read_string, read_u64};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Ethernet,
    Wifi,
    Virtual,
}

impl Kind {
    pub fn icon(self) -> &'static str {
        match self {
            Kind::Ethernet => "network-wired-symbolic",
            Kind::Wifi => "network-wireless-symbolic",
            Kind::Virtual => "network-transmit-receive-symbolic",
        }
    }

    pub fn physical(self) -> bool {
        self != Kind::Virtual
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Iface {
    pub name: String,
    pub kind: Kind,
    pub up: bool,
    pub carrier: bool,
    /// Mb/s, Ethernet only.
    pub speed: Option<u32>,
    /// `<dir>/<name>`, kept so status reads don't rebuild it.
    pub dir: PathBuf,
}

const ARPHRD_LOOPBACK: u64 = 772;
const IFF_UP: u64 = 0x1;

/// `operstate` = up, or `unknown` with IFF_UP (tunnels).
pub fn is_up(dir: &Path) -> bool {
    match read_string(dir.join("operstate")).as_deref() {
        Some("up") => true,
        Some("unknown") => {
            read_string(dir.join("flags")).and_then(|f| u64::from_str_radix(f.trim_start_matches("0x"), 16).ok()).is_some_and(|f| f & IFF_UP != 0)
        }
        _ => false,
    }
}

/// A read error counts as no carrier.
pub fn has_carrier(dir: &Path) -> bool {
    read_u64(dir.join("carrier")) == Some(1)
}

/// Every interface under `root` except loopback, unordered.
pub fn enumerate(root: &Path) -> Vec<Iface> {
    let Ok(entries) = std::fs::read_dir(root) else { return Vec::new() };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let Ok(name) = entry.file_name().into_string() else { continue };
        let dir = root.join(&name);
        if read_u64(dir.join("type")) == Some(ARPHRD_LOOPBACK) || name == "lo" {
            continue;
        }
        let kind = if dir.join("wireless").exists() {
            Kind::Wifi
        } else if dir.join("device").exists() {
            Kind::Ethernet
        } else {
            Kind::Virtual
        };
        let speed = if kind == Kind::Ethernet { read_i64(dir.join("speed")).filter(|s| *s > 0).map(|s| s as u32) } else { None };
        out.push(Iface { kind, up: is_up(&dir), carrier: has_carrier(&dir), speed, name, dir });
    }
    out
}

/// Adapter-list order: physical (Ethernet, then Wi-Fi), then virtual, each
/// alphabetical; `veth*` hidden when there are more than 3 of them.
pub fn ordered(list: &[Iface]) -> Vec<&Iface> {
    let veths = list.iter().filter(|i| i.name.starts_with("veth")).count();
    let mut v: Vec<&Iface> = list.iter().filter(|i| veths <= 3 || !i.name.starts_with("veth")).collect();
    v.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.name.cmp(&b.name)));
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fake(root: &Path, name: &str, files: &[(&str, &str)], dirs: &[&str]) {
        let d = root.join(name);
        fs::create_dir_all(&d).unwrap();
        for (f, v) in files {
            fs::write(d.join(f), v).unwrap();
        }
        for sub in dirs {
            fs::create_dir_all(d.join(sub)).unwrap();
        }
    }

    #[test]
    fn enumerates_and_orders() {
        let root = std::env::temp_dir().join(format!("net-enum-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fake(&root, "lo", &[("type", "772\n"), ("operstate", "unknown\n"), ("flags", "0x9\n")], &[]);
        fake(&root, "wlan0", &[("type", "1\n"), ("operstate", "dormant\n"), ("carrier", "0\n")], &["wireless", "device"]);
        fake(&root, "enp5s0", &[("type", "1\n"), ("operstate", "up\n"), ("carrier", "1\n"), ("speed", "1000\n")], &["device"]);
        fake(&root, "enp6s0", &[("type", "1\n"), ("operstate", "down\n"), ("speed", "-1\n")], &["device"]);
        fake(&root, "tailscale0", &[("type", "65534\n"), ("operstate", "unknown\n"), ("flags", "0x11d1\n"), ("carrier", "1\n")], &[]);
        for i in 0..4 {
            fake(&root, &format!("veth{i}"), &[("type", "1\n"), ("operstate", "up\n")], &[]);
        }
        let list = enumerate(&root);
        assert!(!list.iter().any(|i| i.name == "lo"));
        let names: Vec<&str> = ordered(&list).iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["enp5s0", "enp6s0", "wlan0", "tailscale0"]);
        let get = |n: &str| list.iter().find(|i| i.name == n).unwrap();
        assert_eq!(get("enp5s0").speed, Some(1000));
        assert_eq!(get("enp6s0").speed, None);
        assert!(get("tailscale0").up);
        assert_eq!(get("tailscale0").kind, Kind::Virtual);
        assert_eq!(get("wlan0").kind, Kind::Wifi);
        assert!(!get("wlan0").up);
        fs::remove_dir_all(&root).unwrap();
    }
}
