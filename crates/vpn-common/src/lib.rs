//! Shared by the COSMIC VPN applet and its root helper: the Proton conf
//! parser, the NAT-PMP codec, the status types, the qBittorrent conf editor
//! and the D-Bus names.

pub mod conf;
pub mod natpmp;
pub mod qbitconf;
pub mod status;

/// The helper's D-Bus API (SPEC §12).
pub mod dbus {
    pub const BUS_NAME: &str = "io.github.dc.CosmicVpnHelper";
    pub const OBJECT_PATH: &str = "/io/github/dc/CosmicVpnHelper";
    pub const INTERFACE: &str = "io.github.dc.CosmicVpnHelper1";
    pub const VERSION: u32 = 1;
    /// polkit actions.
    pub const ACTION_CONTROL: &str = "io.github.dc.cosmicvpn.control";
    pub const ACTION_IMPORT: &str = "io.github.dc.cosmicvpn.import";
    /// D-Bus error names, `<INTERFACE>.Error.<name>`.
    pub const ERRORS: [&str; 6] = ["InvalidConf", "NotImported", "PermissionDenied", "QbitRunning", "NetlinkFailed", "WebUi"];
}

/// Names shared by the helper's setup and the applet's leak detection.
pub mod names {
    /// The torrent namespace.
    pub const NETNS: &str = "vpn-p2p";
    pub const NETNS_PATH: &str = "/run/netns/vpn-p2p";
    /// The torrent WireGuard interface.
    pub const P2P_IF: &str = "wg-p2p";
    /// NetworkManager's interface for the web tunnel.
    pub const WEB_IF: &str = "wg-web";
    /// Encrypted torrent packets carry this mark and bypass other VPNs.
    pub const FWMARK: u32 = 0xca6c;
    pub const RULE_PRIORITY: u32 = 100;
    /// The transient unit qBittorrent runs in.
    pub const QBIT_UNIT: &str = "cosmic-vpn-qbittorrent.service";
    /// IPv6 placeholder addresses that make v6 traffic die in the tunnel.
    pub const P2P_V6_PLACEHOLDER: &str = "fd00:ca6c::2/128";
    pub const WEB_V6_PLACEHOLDER: &str = "fd00:ca6c::3/128";
    /// Where the helper keeps the torrent conf (0600 root).
    pub const P2P_CONF: &str = "/etc/cosmic-vpn/p2p.conf";
}

/// Replaces the values of `PrivateKey`, `PresharedKey`, `password=` and
/// `SID=` in text bound for a log.
pub fn redact(s: &str) -> String {
    let mut out = conf::redact(s);
    for needle in ["password=", "SID="] {
        let mut result = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(i) = rest.find(needle) {
            let after = i + needle.len();
            result.push_str(&rest[..after]);
            result.push_str("<redacted>");
            let tail = &rest[after..];
            let end = tail.find(['&', ';', ' ', '\n', '"']).unwrap_or(tail.len());
            rest = &tail[end..];
        }
        result.push_str(rest);
        out = result;
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn redaction() {
        assert_eq!(super::redact("username=admin&password=hunter2&x=1"), "username=admin&password=<redacted>&x=1");
        assert_eq!(super::redact("Set-Cookie: SID=abc123; HttpOnly"), "Set-Cookie: SID=<redacted>; HttpOnly");
        assert_eq!(super::redact("PrivateKey = AAAA\nAddress = 1"), "PrivateKey = REDACTED\nAddress = 1");
    }
}
