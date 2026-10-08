//! What the UI shows, derived from the two tunnels' states: the issue dot,
//! the reticle, the panel chunks and the header caption (SPEC §7.1, the
//! prototype's `vpnIssue`, `vpnIcon`, `vpnWord`, `vpnHeadHTML`).

use vpn_common::status::{PortState, QbitState, Status, TunnelState};

/// The web tunnel, as NetworkManager reports it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Web {
    pub state: TunnelState,
    /// NM's reason, when `state` is `Error`.
    pub error: String,
    /// Seconds since the last handshake (from the helper); `None` if unknown.
    pub handshake_age: Option<u32>,
    pub rx_bps: u64,
    pub tx_bps: u64,
    /// Another VPN connection is active in NetworkManager.
    pub other_vpn: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Issue {
    /// Red dot: a leak or a failed tunnel.
    Error,
    /// Amber dot: stale, or no port forwarding.
    Warning,
}

pub fn issue(p2p: &Status, web: &Web) -> Option<Issue> {
    if p2p.qbit == QbitState::Outside || p2p.state == TunnelState::Error || web.state == TunnelState::Error {
        return Some(Issue::Error);
    }
    if p2p.state == TunnelState::Stale || web.state == TunnelState::Stale || (p2p.state == TunnelState::On && p2p.port_state == PortState::Fail) {
        return Some(Issue::Warning);
    }
    None
}

/// The reticle: first chevron = torrents, second = web; brackets close in
/// when both are up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reticle {
    Off,
    P2p,
    Web,
    Both,
}

impl Reticle {
    pub fn of(p2p: TunnelState, web: TunnelState) -> Self {
        match (p2p.up(), web.up()) {
            (true, true) => Self::Both,
            (true, false) => Self::P2p,
            (false, true) => Self::Web,
            (false, false) => Self::Off,
        }
    }
}

/// How a chunk value is coloured; always with a word too.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Off,
    Busy,
    On,
    Warn,
    Err,
}

/// A panel chunk's value word and tone for a tunnel state (`ON` stays `ON`
/// when stale, in amber).
pub fn chunk(state: TunnelState) -> (&'static str, Tone) {
    match state {
        TunnelState::Off => ("panel-off", Tone::Off),
        TunnelState::Connecting => ("panel-connecting", Tone::Busy),
        TunnelState::On => ("panel-on", Tone::On),
        TunnelState::Stale => ("panel-on", Tone::Warn),
        TunnelState::Error => ("panel-error", Tone::Err),
    }
}

/// The `PORT` chunk: shown only when asked for, torrents are on and a port is mapped.
pub fn port_chunk(p2p: &Status, show_port: bool) -> Option<u16> {
    (show_port && p2p.state == TunnelState::On && p2p.port_state == PortState::Ok && p2p.port != 0).then_some(p2p.port)
}

/// The header caption's `.ftl` key and tone.
pub fn header(p2p: &Status, web: &Web) -> (&'static str, Option<Tone>) {
    match issue(p2p, web) {
        Some(Issue::Error) if p2p.qbit == QbitState::Outside => ("head-leak", Some(Tone::Err)),
        Some(Issue::Error) => ("head-error", Some(Tone::Err)),
        Some(Issue::Warning) => ("head-warn", Some(Tone::Warn)),
        None => match (p2p.state.up(), web.state.up()) {
            (true, true) => ("head-both", Some(Tone::On)),
            (true, false) => ("head-p2p", Some(Tone::On)),
            (false, true) => ("head-web", Some(Tone::On)),
            (false, false) => ("head-none", None),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p2p(state: TunnelState) -> Status {
        Status { state, port_state: PortState::Ok, port: 53186, ..Status::default() }
    }

    fn web(state: TunnelState) -> Web {
        Web { state, ..Web::default() }
    }

    #[test]
    fn issues() {
        assert_eq!(issue(&p2p(TunnelState::On), &web(TunnelState::On)), None);
        assert_eq!(issue(&Status { qbit: QbitState::Outside, ..Status::default() }, &web(TunnelState::Off)), Some(Issue::Error));
        assert_eq!(issue(&p2p(TunnelState::Error), &web(TunnelState::Off)), Some(Issue::Error));
        assert_eq!(issue(&p2p(TunnelState::Off), &web(TunnelState::Error)), Some(Issue::Error));
        assert_eq!(issue(&p2p(TunnelState::Stale), &web(TunnelState::Off)), Some(Issue::Warning));
        assert_eq!(issue(&p2p(TunnelState::Off), &web(TunnelState::Stale)), Some(Issue::Warning));
        let no_port = Status { port_state: PortState::Fail, ..p2p(TunnelState::On) };
        assert_eq!(issue(&no_port, &web(TunnelState::Off)), Some(Issue::Warning));
        // A failed port only matters while the tunnel is on.
        assert_eq!(issue(&Status { port_state: PortState::Fail, ..Status::default() }, &web(TunnelState::Off)), None);
        // Leaks outrank stale.
        assert_eq!(issue(&Status { qbit: QbitState::Outside, ..p2p(TunnelState::Stale) }, &web(TunnelState::Off)), Some(Issue::Error));
    }

    #[test]
    fn reticles() {
        use TunnelState::*;
        assert_eq!(Reticle::of(Off, Off), Reticle::Off);
        assert_eq!(Reticle::of(On, Off), Reticle::P2p);
        assert_eq!(Reticle::of(Stale, Off), Reticle::P2p);
        assert_eq!(Reticle::of(Connecting, On), Reticle::Web);
        assert_eq!(Reticle::of(Error, Stale), Reticle::Web);
        assert_eq!(Reticle::of(On, On), Reticle::Both);
    }

    #[test]
    fn chunks() {
        assert_eq!(chunk(TunnelState::Off), ("panel-off", Tone::Off));
        assert_eq!(chunk(TunnelState::Connecting), ("panel-connecting", Tone::Busy));
        assert_eq!(chunk(TunnelState::On), ("panel-on", Tone::On));
        assert_eq!(chunk(TunnelState::Stale), ("panel-on", Tone::Warn));
        assert_eq!(chunk(TunnelState::Error), ("panel-error", Tone::Err));
        assert_eq!(port_chunk(&p2p(TunnelState::On), true), Some(53186));
        assert_eq!(port_chunk(&p2p(TunnelState::On), false), None);
        assert_eq!(port_chunk(&p2p(TunnelState::Stale), true), None);
        assert_eq!(port_chunk(&Status { port_state: PortState::Fail, ..p2p(TunnelState::On) }, true), None);
    }

    #[test]
    fn header_captions() {
        assert_eq!(header(&p2p(TunnelState::Off), &web(TunnelState::Off)), ("head-none", None));
        assert_eq!(header(&p2p(TunnelState::On), &web(TunnelState::On)).0, "head-both");
        assert_eq!(header(&p2p(TunnelState::On), &web(TunnelState::Off)).0, "head-p2p");
        assert_eq!(header(&p2p(TunnelState::Off), &web(TunnelState::On)).0, "head-web");
        assert_eq!(header(&Status { qbit: QbitState::Outside, ..p2p(TunnelState::On) }, &web(TunnelState::Off)).0, "head-leak");
        assert_eq!(header(&p2p(TunnelState::Off), &web(TunnelState::Error)).0, "head-error");
        assert_eq!(header(&p2p(TunnelState::Stale), &web(TunnelState::Off)).0, "head-warn");
    }
}
