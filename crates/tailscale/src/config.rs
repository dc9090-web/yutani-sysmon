//! Settings, stored with cosmic-config under the App ID (version 1). No
//! secrets: the applet has none.

use cosmic::cosmic_config::{self, CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry};
use serde::{Deserialize, Serialize};

pub const APP_ID: &str = "io.github.dc.CosmicAppletTailscale";

/// What clicking a device copies.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CopyWhat {
    /// Its IPv4 address.
    #[default]
    Address,
    /// Its MagicDNS name.
    Name,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, CosmicConfigEntry)]
#[version = 1]
pub struct Config {
    pub show_offline: bool,
    pub copy: CopyWhat,
}

impl Default for Config {
    fn default() -> Self {
        Self { show_offline: true, copy: CopyWhat::Address }
    }
}

impl Config {
    pub fn load() -> (Option<cosmic_config::Config>, Self) {
        match cosmic_config::Config::new(APP_ID, Self::VERSION) {
            Ok(handler) => {
                let config = Self::get_entry(&handler).unwrap_or_else(|(errors, config)| {
                    for e in errors.iter().filter(|e| e.is_err()) {
                        tracing::warn!("reading config: {e}");
                    }
                    config
                });
                (Some(handler), config)
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
        assert!(d.show_offline);
        assert_eq!(d.copy, CopyWhat::Address);
    }
}
