//! Proton WireGuard `.conf` parsing (SPEC §3). The private key is decoded
//! to validate it and held in a zeroising buffer; nothing here formats it.

use std::fmt;
use std::net::IpAddr;

use base64::Engine;
use base64::engine::{GeneralPurpose, GeneralPurposeConfig};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

/// A `# … = on|off` comment hint.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Hint {
    On,
    Off,
    #[default]
    Unknown,
}

impl Hint {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::On => "on",
            Self::Off => "off",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "on" => Self::On,
            "off" => Self::Off,
            _ => Self::Unknown,
        }
    }
}

/// Why a conf was refused. Each maps to a `.ftl` key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfError {
    NotWireGuard,
    NoKey,
    NoDefaultRoute,
    Scripts,
    Peers,
    NoAddress,
    NoPeerKey,
    NoEndpoint,
}

impl ConfError {
    /// The `.ftl` key for the message.
    pub fn ftl_key(self) -> &'static str {
        match self {
            Self::NotWireGuard => "err-not-wg",
            Self::NoKey => "err-no-key",
            Self::NoDefaultRoute => "err-no-default",
            Self::Scripts => "err-scripts",
            Self::Peers => "err-peers",
            Self::NoAddress => "err-no-address",
            Self::NoPeerKey => "err-no-peer-key",
            Self::NoEndpoint => "err-no-endpoint",
        }
    }
}

impl fmt::Display for ConfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotWireGuard => "Not a WireGuard config",
            Self::NoKey => "Missing or invalid PrivateKey",
            Self::NoDefaultRoute => "AllowedIPs must include 0.0.0.0/0",
            Self::Scripts => "Scripts (PostUp…) are not allowed",
            Self::Peers => "Exactly one [Peer] is required",
            Self::NoAddress => "Missing an IPv4 Address",
            Self::NoPeerKey => "Missing or invalid peer PublicKey",
            Self::NoEndpoint => "Missing or invalid Endpoint",
        })
    }
}

impl std::error::Error for ConfError {}

/// What the applet keeps about a conf: no key material (SPEC §9).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfSummary {
    pub file_name: String,
    pub server_label: String,
    pub endpoint: String,
    pub addr_v4: String,
    pub has_v6: bool,
    pub dns: Vec<String>,
    pub natpmp_hint: Hint,
    pub moderate_nat_hint: Hint,
    /// Set after the first NAT-PMP round: `Some(true)` once a port was mapped.
    pub natpmp_capable: Option<bool>,
}

/// An `IP/prefix`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cidr {
    pub addr: IpAddr,
    pub prefix: u8,
}

impl Cidr {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        let (a, p) = match s.split_once('/') {
            Some((a, p)) => (a, Some(p)),
            None => (s, None),
        };
        let addr: IpAddr = a.trim().parse().ok()?;
        let max = if addr.is_ipv4() { 32 } else { 128 };
        let prefix = match p {
            Some(p) => p.trim().parse::<u8>().ok().filter(|p| *p <= max)?,
            None => max,
        };
        Some(Self { addr, prefix })
    }

    pub fn is_default(&self) -> bool {
        self.prefix == 0 && self.addr.is_unspecified()
    }
}

impl fmt::Display for Cidr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.addr, self.prefix)
    }
}

/// `host:port`, `a.b.c.d:port` or `[v6]:port`. Hostnames are kept for the
/// helper to resolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
}

impl Endpoint {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        let (host, port) = if let Some(rest) = s.strip_prefix('[') {
            let (h, p) = rest.split_once("]:")?;
            h.parse::<std::net::Ipv6Addr>().ok()?;
            (h, p)
        } else {
            let (h, p) = s.rsplit_once(':')?;
            if h.contains(':') {
                return None;
            }
            (h, p)
        };
        let port = port.parse::<u16>().ok().filter(|p| *p != 0)?;
        let valid_host = !host.is_empty() && host.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'));
        valid_host.then(|| Self { host: host.to_owned(), port })
    }

    /// The host as an IP, if it is one.
    pub fn ip(&self) -> Option<IpAddr> {
        self.host.parse().ok()
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.host.contains(':') { write!(f, "[{}]:{}", self.host, self.port) } else { write!(f, "{}:{}", self.host, self.port) }
    }
}

/// Standard base64 that also accepts non-zero trailing bits (the fixtures'
/// dummy keys have them; real keys don't).
const B64: GeneralPurpose = GeneralPurpose::new(&base64::alphabet::STANDARD, GeneralPurposeConfig::new().with_decode_allow_trailing_bits(true));

/// A 32-byte WireGuard key that zeroises itself and never prints.
#[derive(Clone, PartialEq, Eq)]
pub struct Key(pub Zeroizing<[u8; 32]>);

impl Key {
    fn decode(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.len() != 44 {
            return None;
        }
        let mut buf = Zeroizing::new(Vec::with_capacity(33));
        B64.decode_vec(s, &mut buf).ok()?;
        let bytes: [u8; 32] = buf.as_slice().try_into().ok()?;
        Some(Self(Zeroizing::new(bytes)))
    }

    pub fn bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Key(<redacted>)")
    }
}

/// A fully parsed conf, key included. Only the helper and the NM import hold one.
#[derive(Debug, Clone)]
pub struct Conf {
    pub summary: ConfSummary,
    pub private_key: Key,
    pub addresses: Vec<Cidr>,
    pub dns: Vec<IpAddr>,
    pub mtu: Option<u16>,
    pub peer_public_key: [u8; 32],
    pub preshared_key: Option<Key>,
    pub endpoint: Endpoint,
    pub allowed_ips: Vec<Cidr>,
    pub keepalive: Option<u16>,
}

/// The structure of a conf, without its private key (for `--check-conf`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Structure {
    pub summary: ConfSummary,
    /// The `PrivateKey` line is present (its value isn't checked).
    pub has_private_key: bool,
}

/// `key = value` lines (keys lower-cased) and `#` comments of one section.
type Entries<'a> = Vec<(String, &'a str)>;

#[derive(Default)]
struct Raw<'a> {
    interface: Entries<'a>,
    interface_comments: Vec<&'a str>,
    peers: Vec<(Entries<'a>, Vec<&'a str>)>,
}

/// What both entry points read: everything but the private key.
struct Parts {
    summary: ConfSummary,
    addresses: Vec<Cidr>,
    dns: Vec<IpAddr>,
    endpoint: Endpoint,
    allowed_ips: Vec<Cidr>,
    peer_public_key: [u8; 32],
}

impl<'a> Raw<'a> {
    fn get(entries: &[(String, &'a str)], key: &str) -> Option<&'a str> {
        entries.iter().find(|(k, _)| k == key).map(|(_, v)| *v)
    }
}

const SCRIPT_KEYS: [&str; 4] = ["preup", "postup", "predown", "postdown"];

fn split_raw(text: &str) -> Result<Raw<'_>, ConfError> {
    #[derive(PartialEq)]
    enum Sec {
        None,
        Interface,
        Peer,
        Other,
    }
    let mut raw = Raw::default();
    let mut sec = Sec::None;
    let mut seen_interface = false;
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if let Some(c) = t.strip_prefix('#').or_else(|| t.strip_prefix(';')) {
            match sec {
                Sec::Interface => raw.interface_comments.push(c.trim()),
                Sec::Peer => raw.peers.last_mut().expect("in a peer").1.push(c.trim()),
                _ => {}
            }
            continue;
        }
        if t.starts_with('[') && t.ends_with(']') {
            sec = match t[1..t.len() - 1].trim().to_ascii_lowercase().as_str() {
                "interface" => {
                    seen_interface = true;
                    Sec::Interface
                }
                "peer" => {
                    raw.peers.push((Vec::new(), Vec::new()));
                    Sec::Peer
                }
                _ => Sec::Other,
            };
            continue;
        }
        let Some((k, v)) = t.split_once('=') else { return Err(ConfError::NotWireGuard) };
        let key = k.trim().to_ascii_lowercase();
        // Inline comments: base64 keys and addresses never contain '#'.
        let value = v.split('#').next().unwrap_or("").trim();
        match sec {
            Sec::Interface => raw.interface.push((key, value)),
            Sec::Peer => raw.peers.last_mut().expect("in a peer").0.push((key, value)),
            Sec::None => return Err(ConfError::NotWireGuard),
            Sec::Other => {}
        }
    }
    if !seen_interface {
        return Err(ConfError::NotWireGuard);
    }
    let scripts = raw.interface.iter().chain(raw.peers.iter().flat_map(|p| p.0.iter())).any(|(k, _)| SCRIPT_KEYS.contains(&k.as_str()));
    if scripts {
        return Err(ConfError::Scripts);
    }
    if raw.peers.len() != 1 {
        return Err(ConfError::Peers);
    }
    Ok(raw)
}

/// `# Name = value` in the interface comments.
fn hint(comments: &[&str], name: &str) -> Hint {
    comments.iter().filter_map(|c| c.split_once('=')).find(|(k, _)| k.trim().eq_ignore_ascii_case(name)).map_or(Hint::Unknown, |(_, v)| Hint::parse(v))
}

fn list(v: Option<&str>) -> impl Iterator<Item = &str> {
    v.unwrap_or("").split(',').map(str::trim).filter(|s| !s.is_empty())
}

/// Everything but the private key; shared by both entry points.
fn structure(raw: &Raw<'_>, file_name: &str) -> Result<Parts, ConfError> {
    let (peer, peer_comments) = &raw.peers[0];
    let addresses: Vec<Cidr> = list(Raw::get(&raw.interface, "address")).map(Cidr::parse).collect::<Option<_>>().ok_or(ConfError::NoAddress)?;
    let v4 = addresses.iter().find(|c| c.addr.is_ipv4()).ok_or(ConfError::NoAddress)?;
    let peer_key = Raw::get(peer, "publickey").and_then(Key::decode).ok_or(ConfError::NoPeerKey)?;
    let endpoint = Raw::get(peer, "endpoint").and_then(Endpoint::parse).ok_or(ConfError::NoEndpoint)?;
    let allowed: Vec<Cidr> = list(Raw::get(peer, "allowedips")).filter_map(Cidr::parse).collect();
    if !allowed.iter().any(|c| c.is_default() && c.addr.is_ipv4()) {
        return Err(ConfError::NoDefaultRoute);
    }
    let dns: Vec<IpAddr> = list(Raw::get(&raw.interface, "dns")).filter_map(|s| s.parse().ok()).collect();
    let stem = file_name.rsplit('/').next().unwrap_or(file_name);
    let stem = stem.strip_suffix(".conf").unwrap_or(stem);
    let server_label = peer_comments.first().map(|c| c.to_string()).filter(|c| !c.is_empty()).unwrap_or_else(|| stem.to_owned());
    let summary = ConfSummary {
        file_name: file_name.rsplit('/').next().unwrap_or(file_name).to_owned(),
        server_label,
        endpoint: endpoint.to_string(),
        addr_v4: v4.to_string(),
        has_v6: addresses.iter().any(|c| c.addr.is_ipv6()),
        dns: dns.iter().map(ToString::to_string).collect(),
        natpmp_hint: hint(&raw.interface_comments, "NAT-PMP (Port Forwarding)"),
        moderate_nat_hint: hint(&raw.interface_comments, "Moderate NAT"),
        natpmp_capable: None,
    };
    Ok(Parts { summary, addresses, dns, endpoint, allowed_ips: allowed, peer_public_key: *peer_key.bytes() })
}

/// Parses a conf, private key included.
pub fn parse(text: &str, file_name: &str) -> Result<Conf, ConfError> {
    let raw = split_raw(text)?;
    let private_key = Raw::get(&raw.interface, "privatekey").and_then(Key::decode).ok_or(ConfError::NoKey)?;
    let Parts { summary, addresses, dns, endpoint, allowed_ips, peer_public_key } = structure(&raw, file_name)?;
    let (peer, _) = &raw.peers[0];
    Ok(Conf {
        summary,
        private_key,
        addresses,
        dns,
        mtu: Raw::get(&raw.interface, "mtu").and_then(|v| v.parse().ok()),
        peer_public_key,
        preshared_key: Raw::get(peer, "presharedkey").and_then(Key::decode),
        endpoint,
        allowed_ips,
        keepalive: Raw::get(peer, "persistentkeepalive").and_then(|v| v.parse().ok()),
    })
}

/// Checks a conf's structure and accepts `PrivateKey = REDACTED`, so a real
/// conf can be checked without sharing its key.
pub fn parse_structure(text: &str, file_name: &str) -> Result<Structure, ConfError> {
    let raw = split_raw(text)?;
    let has_private_key = Raw::get(&raw.interface, "privatekey").is_some_and(|v| !v.is_empty());
    let summary = structure(&raw, file_name)?.summary;
    Ok(Structure { summary, has_private_key })
}

/// A copy of `text` with every `PrivateKey` / `PresharedKey` value replaced,
/// for anything that might be shown or logged.
pub fn redact(text: &str) -> String {
    text.lines()
        .map(|l| {
            let key = l.split('=').next().unwrap_or("").trim().to_ascii_lowercase();
            if (key == "privatekey" || key == "presharedkey") && l.contains('=') {
                format!("{}= REDACTED", l.split('=').next().unwrap_or(""))
            } else {
                l.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!("{}/tests/fixtures/conf/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    fn expected() -> serde_json::Value {
        serde_json::from_str(&fixture("expected.json")).unwrap()
    }

    #[test]
    fn every_fixture_matches_expected_json() {
        let exp = expected();
        for (name, want) in exp.as_object().unwrap() {
            let got = parse(&fixture(name), name);
            if want["ok"].as_bool().unwrap() {
                let c = got.unwrap_or_else(|e| panic!("{name}: {e}"));
                let s = &c.summary;
                let check = |field: &str, value: serde_json::Value| {
                    if let Some(w) = want.get(field) {
                        assert_eq!(&value, w, "{name}: {field}");
                    }
                };
                check("server_label", s.server_label.clone().into());
                check("endpoint", s.endpoint.clone().into());
                check("addr_v4", s.addr_v4.clone().into());
                check("has_v6", s.has_v6.into());
                check("dns", serde_json::to_value(&s.dns).unwrap());
                check("natpmp_hint", s.natpmp_hint.as_str().into());
                check("moderate_nat_hint", s.moderate_nat_hint.as_str().into());
            } else {
                let e = got.expect_err(name);
                assert_eq!(e.ftl_key(), want["error"].as_str().unwrap(), "{name}");
            }
        }
    }

    #[test]
    fn full_parse_details() {
        let c = parse(&fixture("p2p_natpmp_on_v6.conf"), "proton-nl-256-p2p.conf").unwrap();
        assert_eq!(c.private_key.bytes(), &[0u8; 32]);
        assert_eq!(c.peer_public_key[..3], [0x04, 0x10, 0x41]);
        assert_eq!(c.endpoint, Endpoint { host: "185.107.56.23".into(), port: 51820 });
        assert_eq!(c.addresses.len(), 2);
        assert_eq!(c.allowed_ips.iter().map(ToString::to_string).collect::<Vec<_>>(), ["0.0.0.0/0", "::/0"]);
        assert_eq!(c.summary.file_name, "proton-nl-256-p2p.conf");
        assert_eq!((c.mtu, c.keepalive, c.preshared_key.is_none()), (None, None, true));
        // The key never prints.
        assert!(!format!("{c:?}").contains("AAAA"));
    }

    #[test]
    fn structure_mode_accepts_a_redacted_key() {
        let s = parse_structure(&fixture("redacted_key.conf"), "redacted_key.conf").unwrap();
        assert!(s.has_private_key);
        assert_eq!(s.summary.server_label, "NL#256");
        assert_eq!(s.summary.natpmp_hint, Hint::On);
        // It still refuses what the full parser refuses.
        assert_eq!(parse_structure(&fixture("invalid_postup.conf"), "x"), Err(ConfError::Scripts));
        assert_eq!(parse_structure(&fixture("invalid_split_tunnel.conf"), "x"), Err(ConfError::NoDefaultRoute));
    }

    #[test]
    fn edge_cases() {
        let base = fixture("p2p_natpmp_on.conf");
        // Case-insensitive keys and sections.
        let upper = base.replace("[Interface]", "[INTERFACE]").replace("AllowedIPs", "allowedips");
        assert!(parse(&upper, "x.conf").is_ok());
        // Hostname and IPv6 endpoints.
        let host = base.replace("185.107.56.23:51820", "nl-256.protonvpn.net:51820");
        assert_eq!(parse(&host, "x").unwrap().endpoint.host, "nl-256.protonvpn.net");
        let v6 = base.replace("185.107.56.23:51820", "[2a07:b944::1]:51820");
        assert_eq!(parse(&v6, "x").unwrap().summary.endpoint, "[2a07:b944::1]:51820");
        assert_eq!(parse(&base.replace("185.107.56.23:51820", "185.107.56.23"), "x").unwrap_err(), ConfError::NoEndpoint);
        // A short key, a missing address, a v6-only address.
        assert_eq!(parse(&base.replace("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=", "AAAA"), "x").unwrap_err(), ConfError::NoKey);
        assert_eq!(parse(&base.replace("Address = 10.2.0.2/32\n", ""), "x").unwrap_err(), ConfError::NoAddress);
        assert_eq!(parse(&base.replace("10.2.0.2/32", "fd00::2/128"), "x").unwrap_err(), ConfError::NoAddress);
        // Any script key anywhere.
        assert_eq!(parse(&base.replace("[Peer]", "[Peer]\nPreDown = true"), "x").unwrap_err(), ConfError::Scripts);
        assert_eq!(parse("", "x").unwrap_err(), ConfError::NotWireGuard);
    }

    #[test]
    fn redaction() {
        let r = redact(&fixture("p2p_natpmp_on.conf"));
        assert!(r.contains("PrivateKey = REDACTED"));
        assert!(!r.contains("AAAAAAAA"));
        assert!(r.contains("PublicKey = BBBB"));
    }
}
