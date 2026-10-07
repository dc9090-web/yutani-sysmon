//! Settings, stored with cosmic-config under the App ID (version 1).

use cosmic::cosmic_config::{self, CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry};
use serde::{Deserialize, Serialize};

pub const APP_ID: &str = "io.github.dc.CosmicAppletNetTraffic";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisplayMode {
    Numbers,
    Graph,
    #[default]
    Both,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Indicator {
    Arrows,
    Triangles,
    #[default]
    Chevrons,
    RxTxIcons,
    Bar,
    RxTx,
}

impl Indicator {
    pub const ALL: [Self; 6] = [Self::Arrows, Self::Triangles, Self::Chevrons, Self::RxTxIcons, Self::Bar, Self::RxTx];
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdapterChoice {
    #[default]
    Automatic,
    Named(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, CosmicConfigEntry)]
#[version = 1]
pub struct Config {
    pub mode: DisplayMode,
    pub indicator: Indicator,
    pub adapter: AdapterChoice,
}

impl Config {
    /// The handler and the stored settings, or the defaults.
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
