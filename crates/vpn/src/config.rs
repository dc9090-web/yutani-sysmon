//! Settings, stored with cosmic-config under the App ID (SPEC §9, version 1).
//! No secrets: the private keys are with the helper and NetworkManager, the
//! Web UI password is in the keyring.

use cosmic::cosmic_config::{self, CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry};
use serde::{Deserialize, Serialize};
use vpn_common::conf::ConfSummary;

pub const APP_ID: &str = "io.github.dc.CosmicAppletVpn";

/// The panel icon's look.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum IconStyle {
    /// The single-ink reticle, like every other symbolic icon.
    #[default]
    Mono,
    /// Cyan with a glow; magenta when there's an issue.
    Neon,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, CosmicConfigEntry)]
#[version = 1]
pub struct Config {
    pub p2p_conf: Option<ConfSummary>,
    pub web_conf: Option<ConfSummary>,
    pub web_nm_uuid: Option<String>,
    pub launch_qbit: bool,
    pub quit_qbit: bool,
    pub auto_port: bool,
    pub webui_port: u16,
    /// Empty: qBittorrent's "Bypass authentication for clients on localhost".
    pub webui_user: String,
    pub show_port: bool,
    pub restore: bool,
    pub last_p2p: bool,
    pub last_web: bool,
    /// Overrides the detected qBittorrent command.
    pub qbit_command: Option<String>,
    pub icon_style: IconStyle,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            p2p_conf: None,
            web_conf: None,
            web_nm_uuid: None,
            launch_qbit: true,
            quit_qbit: true,
            auto_port: true,
            webui_port: 8080,
            webui_user: "admin".to_owned(),
            show_port: true,
            restore: true,
            last_p2p: false,
            last_web: false,
            qbit_command: None,
            icon_style: IconStyle::Mono,
        }
    }
}

impl Config {
    /// A Web UI port of 0 becomes 8080; a blank command override is none.
    pub fn enforce(mut self) -> Self {
        if self.webui_port == 0 {
            self.webui_port = 8080;
        }
        if self.qbit_command.as_ref().is_some_and(|c| c.trim().is_empty()) {
            self.qbit_command = None;
        }
        self
    }

    pub fn load() -> (Option<cosmic_config::Config>, Self) {
        match cosmic_config::Config::new(APP_ID, Self::VERSION) {
            Ok(handler) => {
                let config = Self::get_entry(&handler).unwrap_or_else(|(errors, config)| {
                    for e in errors.iter().filter(|e| e.is_err()) {
                        tracing::warn!("reading config: {e}");
                    }
                    config
                });
                (Some(handler), config.enforce())
            }
            Err(e) => {
                tracing::error!("opening config: {e}");
                (None, Self::default())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let d = Config::default();
        assert!(d.launch_qbit && d.quit_qbit && d.auto_port && d.show_port && d.restore);
        assert!(!d.last_p2p && !d.last_web);
        assert_eq!((d.webui_port, d.webui_user.as_str(), d.icon_style), (8080, "admin", IconStyle::Mono));
        assert_eq!(d.clone().enforce(), d);
        let odd = Config { webui_port: 0, qbit_command: Some("  ".into()), ..Config::default() }.enforce();
        assert_eq!((odd.webui_port, odd.qbit_command), (8080, None));
    }
}
