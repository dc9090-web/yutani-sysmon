//! `VPN_DEMO=1` (feature `demo`): no helper, no NetworkManager, no configs;
//! the applet steps through every state every 8 s.

use vpn_common::conf::{self, ConfSummary};
use vpn_common::status::{ListenApplied, PortState, QbitState, Status, TunnelState};

use crate::app::{Page, WebUiTest};
use crate::config::IconStyle;
use crate::model::Web;

pub const STEP: std::time::Duration = std::time::Duration::from_secs(8);

pub fn enabled() -> bool {
    std::env::var_os("VPN_DEMO").is_some_and(|v| !v.is_empty() && v != "0")
}

/// `VPN_DEMO_SCENE=n` starts on scene `n`.
pub fn first() -> usize {
    std::env::var("VPN_DEMO_SCENE").ok().and_then(|s| s.parse().ok()).unwrap_or(0) % SCENES
}

/// `VPN_SHOT=path`: save the window as a PAM image after the first frames, then exit.
pub fn shot_path() -> Option<std::path::PathBuf> {
    std::env::var_os("VPN_SHOT").filter(|v| !v.is_empty()).map(Into::into)
}

pub fn save_shot(path: &std::path::Path, shot: &cosmic::iced::window::Screenshot) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    write!(f, "P7\nWIDTH {}\nHEIGHT {}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n", shot.size.width, shot.size.height)?;
    f.write_all(&shot.rgba)?;
    f.flush()
}

fn summary(text: &str, file: &str) -> ConfSummary {
    conf::parse(text, file).expect("fixture parses").summary
}

pub struct Scene {
    pub name: &'static str,
    pub p2p: Status,
    pub web: Web,
    pub leak: bool,
    pub helper: bool,
    pub p2p_conf: Option<ConfSummary>,
    pub web_conf: Option<ConfSummary>,
    pub icon_style: IconStyle,
    pub page: Page,
    pub test: Option<WebUiTest>,
    pub confirm: bool,
    pub p2p_wanted: bool,
}

pub const SCENES: usize = 13;

pub fn scene(i: usize) -> Scene {
    let p2p_conf = summary(include_str!("../../vpn-common/tests/fixtures/conf/p2p_natpmp_on.conf"), "proton-nl-256-p2p.conf");
    let web_conf = summary(include_str!("../../vpn-common/tests/fixtures/conf/web_ch12.conf"), "proton-ch-12.conf");
    let on = Status {
        state: TunnelState::On,
        handshake_age: 12,
        rx_bps: 4_800_000,
        tx_bps: 610_000,
        port_state: PortState::Ok,
        port: 53186,
        port_renewed_age: 12,
        qbit: QbitState::Vpn,
        listen_port_applied: ListenApplied::Auto,
        ..Status::default()
    };
    let web_on = Web { state: TunnelState::On, handshake_age: Some(31), rx_bps: 1_200_000, tx_bps: 240_000, ..Web::default() };
    let mut s = Scene {
        name: "both-on",
        p2p: on.clone(),
        web: web_on.clone(),
        leak: false,
        helper: true,
        p2p_conf: Some(p2p_conf),
        web_conf: Some(web_conf),
        icon_style: IconStyle::Mono,
        page: Page::Main,
        test: None,
        confirm: false,
        p2p_wanted: true,
    };
    match i % SCENES {
        0 => {}
        1 => {
            s.name = "off";
            s.p2p = Status::default();
            s.web = Web::default();
            s.p2p_wanted = false;
        }
        2 => {
            s.name = "connecting";
            s.p2p = Status { state: TunnelState::Connecting, ..Status::default() };
            s.web = Web::default();
        }
        3 => {
            s.name = "torrents-on";
            s.web = Web::default();
        }
        4 => {
            s.name = "leak";
            s.leak = true;
            s.web = Web::default();
            s.p2p.listen_port_applied = ListenApplied::None;
        }
        5 => {
            s.name = "no-port-forwarding";
            s.p2p.port_state = PortState::Fail;
            s.p2p.port = 0;
            s.p2p.listen_port_applied = ListenApplied::None;
        }
        6 => {
            s.name = "stale";
            s.p2p.state = TunnelState::Stale;
            s.p2p.handshake_age = 200;
            s.web = Web::default();
        }
        7 => {
            s.name = "web-failed";
            s.p2p = Status::default();
            s.p2p_wanted = false;
            s.web = Web { state: TunnelState::Error, error: "NetworkManager: activation failed (no secrets)".into(), ..Web::default() };
        }
        8 => {
            s.name = "webui-off";
            s.p2p.listen_port_applied = ListenApplied::Manual;
            s.web = Web::default();
        }
        9 => {
            s.name = "neon";
            s.icon_style = IconStyle::Neon;
        }
        10 => {
            s.name = "settings";
            s.page = Page::Settings;
            s.p2p = Status::default();
            s.web = Web::default();
            s.test = Some(WebUiTest::Ok);
        }
        11 => {
            s.name = "helper-missing";
            s.helper = false;
            s.p2p = Status::default();
            s.web = Web::default();
        }
        _ => {
            s.name = "first-run";
            s.p2p_conf = None;
            s.web_conf = None;
            s.p2p = Status::default();
            s.web = Web::default();
        }
    }
    s
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_scene_builds() {
        let names: Vec<_> = (0..super::SCENES).map(|i| super::scene(i).name).collect();
        for n in ["off", "connecting", "both-on", "leak", "stale", "no-port-forwarding", "web-failed", "helper-missing", "first-run", "settings", "neon"] {
            assert!(names.contains(&n), "{n}");
        }
    }
}
