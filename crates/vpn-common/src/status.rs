//! The helper's status dictionary (SPEC §4.4) and the handshake → state
//! mapping (SPEC §4.1).

use std::collections::HashMap;

use zvariant::{OwnedValue, Value};

/// No handshake for this long while up: stale.
pub const STALE_SECS: u64 = 180;
/// No first handshake within this long: error.
pub const HANDSHAKE_TIMEOUT_SECS: u64 = 15;

macro_rules! str_enum {
    ($(#[$m:meta])* $name:ident { $($v:ident = $s:literal),* $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
        pub enum $name { #[default] $($v),* }
        impl $name {
            pub fn as_str(self) -> &'static str { match self { $(Self::$v => $s),* } }
            pub fn parse(s: &str) -> Option<Self> { match s { $($s => Some(Self::$v),)* _ => None } }
        }
    };
}

str_enum!(
    /// A tunnel's state.
    TunnelState { Off = "off", Connecting = "connecting", On = "on", Stale = "stale", Error = "error" }
);
str_enum!(
    /// NAT-PMP port forwarding.
    PortState { Off = "off", Req = "req", Ok = "ok", Fail = "fail" }
);
str_enum!(
    /// Where qBittorrent runs. `Outside` is only ever set by the applet.
    QbitState { Stopped = "stopped", Starting = "starting", Vpn = "vpn", Outside = "outside" }
);
str_enum!(
    /// How qBittorrent's listening port was set.
    ListenApplied { None = "none", Auto = "auto", Conf = "conf", Manual = "manual", Failed = "failed" }
);

impl TunnelState {
    /// `on` or `stale`: traffic can flow.
    pub fn up(self) -> bool {
        matches!(self, Self::On | Self::Stale)
    }
}

/// The torrent tunnel's status, as `StatusChanged(a{sv})` carries it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Status {
    pub state: TunnelState,
    pub error: String,
    pub handshake_age: u32,
    pub rx_bps: u64,
    pub tx_bps: u64,
    pub port_state: PortState,
    pub port: u16,
    pub port_renewed_age: u32,
    pub qbit: QbitState,
    pub listen_port_applied: ListenApplied,
    /// qBittorrent's exit code when its unit failed (for the toast); 0 otherwise.
    pub qbit_exit: i32,
}

impl Status {
    pub fn to_dict(&self) -> HashMap<String, OwnedValue> {
        let s = |v: &str| OwnedValue::try_from(Value::from(v.to_owned())).expect("strings convert");
        HashMap::from([
            ("state".into(), s(self.state.as_str())),
            ("error".into(), s(&self.error)),
            ("handshake_age".into(), OwnedValue::from(self.handshake_age)),
            ("rx_bps".into(), OwnedValue::from(self.rx_bps)),
            ("tx_bps".into(), OwnedValue::from(self.tx_bps)),
            ("port_state".into(), s(self.port_state.as_str())),
            ("port".into(), OwnedValue::from(self.port)),
            ("port_renewed_age".into(), OwnedValue::from(self.port_renewed_age)),
            ("qbit".into(), s(self.qbit.as_str())),
            ("listen_port_applied".into(), s(self.listen_port_applied.as_str())),
            ("qbit_exit".into(), OwnedValue::from(self.qbit_exit)),
        ])
    }

    /// Reads a dictionary; missing or mistyped fields keep their defaults.
    pub fn from_dict(d: &HashMap<String, OwnedValue>) -> Self {
        let text = |k: &str| d.get(k).and_then(|v| <&str>::try_from(v).ok()).unwrap_or_default();
        let u32_ = |k: &str| d.get(k).and_then(|v| u32::try_from(v).ok()).unwrap_or_default();
        let u64_ = |k: &str| d.get(k).and_then(|v| u64::try_from(v).ok()).unwrap_or_default();
        Self {
            state: TunnelState::parse(text("state")).unwrap_or_default(),
            error: text("error").to_owned(),
            handshake_age: u32_("handshake_age"),
            rx_bps: u64_("rx_bps"),
            tx_bps: u64_("tx_bps"),
            port_state: PortState::parse(text("port_state")).unwrap_or_default(),
            port: d.get("port").and_then(|v| u16::try_from(v).ok()).unwrap_or_default(),
            port_renewed_age: u32_("port_renewed_age"),
            qbit: QbitState::parse(text("qbit")).unwrap_or_default(),
            listen_port_applied: ListenApplied::parse(text("listen_port_applied")).unwrap_or_default(),
            qbit_exit: d.get("qbit_exit").and_then(|v| i32::try_from(v).ok()).unwrap_or_default(),
        }
    }
}

/// The state of a tunnel that's switched on, from its latest handshake
/// (0 = none yet), the time now and when it was brought up (all Unix seconds).
pub fn classify(latest_handshake: u64, now: u64, up_since: u64) -> TunnelState {
    if latest_handshake == 0 {
        return if now.saturating_sub(up_since) < HANDSHAKE_TIMEOUT_SECS { TunnelState::Connecting } else { TunnelState::Error };
    }
    if now.saturating_sub(latest_handshake) > STALE_SECS { TunnelState::Stale } else { TunnelState::On }
}

/// One peer line of `wg show <if> dump`: (latest handshake, rx bytes, tx bytes).
/// Used by tests and the demo; the helper reads the same fields over netlink.
pub fn wg_dump_peer(dump: &str) -> Option<(u64, u64, u64)> {
    let line = dump.lines().nth(1)?;
    let f: Vec<&str> = line.split('\t').collect();
    if f.len() < 8 {
        return None;
    }
    Some((f[4].parse().ok()?, f[5].parse().ok()?, f[6].parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!("{}/tests/fixtures/wg/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    #[test]
    fn wg_fixtures_map_as_documented() {
        let (hs, rx, tx) = wg_dump_peer(&fixture("dump_connected.txt")).unwrap();
        assert_eq!((hs, rx, tx), (1_791_437_800, 48_123_904, 6_123_520));
        let up = 1_791_437_000;
        assert_eq!(classify(hs, 1_791_437_812, up), TunnelState::On);
        assert_eq!(1_791_437_812 - hs, 12);
        assert_eq!(classify(hs, 1_791_438_000, up), TunnelState::Stale);
        let (none, ..) = wg_dump_peer(&fixture("dump_no_handshake.txt")).unwrap();
        assert_eq!(none, 0);
        assert_eq!(classify(0, up + 14, up), TunnelState::Connecting);
        assert_eq!(classify(0, up + 15, up), TunnelState::Error);
        // Exactly 180 s is still on.
        assert_eq!(classify(1000, 1180, 0), TunnelState::On);
    }

    #[test]
    fn dict_round_trip() {
        let s = Status {
            state: TunnelState::On,
            error: String::new(),
            handshake_age: 12,
            rx_bps: 4_800_000,
            tx_bps: 610_000,
            port_state: PortState::Ok,
            port: 53186,
            port_renewed_age: 12,
            qbit: QbitState::Vpn,
            listen_port_applied: ListenApplied::Auto,
            qbit_exit: 0,
        };
        assert_eq!(Status::from_dict(&s.to_dict()), s);
        assert_eq!(Status::from_dict(&HashMap::new()), Status::default());
        assert_eq!(TunnelState::parse("stale"), Some(TunnelState::Stale));
        assert!(TunnelState::Stale.up() && !TunnelState::Connecting.up());
    }
}
