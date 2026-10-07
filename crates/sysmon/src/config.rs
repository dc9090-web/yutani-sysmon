//! Settings, stored with cosmic-config under the App ID (version 1).

use cosmic::cosmic_config::{self, CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry};
use serde::{Deserialize, Serialize};

pub const APP_ID: &str = "io.github.dc.CosmicAppletSysMon";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PanelStyle {
    #[default]
    Numbers,
    Graph,
    Both,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiskChoice {
    #[default]
    All,
    Named(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GpuChoice {
    #[default]
    Auto,
    Slot(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, CosmicConfigEntry)]
#[version = 1]
pub struct Config {
    pub show_cpu: bool,
    /// Ignored when there is no amdgpu card.
    pub show_gpu: bool,
    pub show_mem: bool,
    pub show_disk: bool,
    pub style: PanelStyle,
    pub disk: DiskChoice,
    pub gpu: GpuChoice,
}

impl Default for Config {
    fn default() -> Self {
        Self { show_cpu: true, show_gpu: true, show_mem: true, show_disk: false, style: PanelStyle::Numbers, disk: DiskChoice::All, gpu: GpuChoice::Auto }
    }
}

impl Config {
    /// At least one metric stays on.
    pub fn enforce(mut self) -> Self {
        if !(self.show_cpu || self.show_gpu || self.show_mem || self.show_disk) {
            self.show_cpu = true;
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

    pub fn choice(&self) -> crate::sample::Choice {
        crate::sample::Choice {
            disk: match &self.disk {
                DiskChoice::Named(d) => Some(d.clone()),
                DiskChoice::All => None,
            },
            gpu: match &self.gpu {
                GpuChoice::Slot(s) => Some(s.clone()),
                GpuChoice::Auto => None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_invariant() {
        let d = Config::default();
        assert!(d.show_cpu && d.show_gpu && d.show_mem && !d.show_disk);
        let none = Config { show_cpu: false, show_gpu: false, show_mem: false, show_disk: false, ..d };
        assert!(none.enforce().show_cpu);
    }
}
