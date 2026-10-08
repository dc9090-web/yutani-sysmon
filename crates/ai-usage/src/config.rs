//! Settings, stored with cosmic-config under the App ID (version 1).

use cosmic::cosmic_config::{self, CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry};
use serde::{Deserialize, Serialize};

pub const APP_ID: &str = "io.github.dc.CosmicAppletAiUsage";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PanelStyle {
    Percent,
    #[default]
    Bars,
    Both,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Amount {
    #[default]
    Used,
    Left,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResetFormat {
    #[default]
    Relative,
    Absolute,
}

/// The COSMIC time applet's settings; only the 12/24-hour choice is read.
pub const TIME_APPLET: &str = "com.system76.CosmicAppletTime";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, CosmicConfigEntry)]
#[version = 1]
pub struct TimeConfig {
    pub military_time: bool,
}

impl Default for TimeConfig {
    /// 24-hour time when the time applet's setting can't be read.
    fn default() -> Self {
        Self { military_time: true }
    }
}

impl TimeConfig {
    pub fn load() -> Self {
        cosmic_config::Config::new(TIME_APPLET, Self::VERSION).ok().map(|h| Self::get_entry(&h).unwrap_or_else(|(_, c)| c)).unwrap_or_default()
    }
}

/// The refresh intervals offered, in minutes.
pub const REFRESH_CHOICES: [u8; 3] = [1, 5, 15];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, CosmicConfigEntry)]
#[version = 1]
pub struct Config {
    pub show_session: bool,
    pub show_weekly: bool,
    pub show_fable: bool,
    pub show_session_reset: bool,
    pub style: PanelStyle,
    pub amount: Amount,
    pub reset_format: ResetFormat,
    pub refresh_minutes: u8,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            show_session: true,
            show_weekly: true,
            show_fable: true,
            show_session_reset: false,
            style: PanelStyle::Bars,
            amount: Amount::Used,
            reset_format: ResetFormat::Relative,
            refresh_minutes: 5,
        }
    }
}

impl Config {
    /// At least one window stays on; an unknown interval becomes 5 min.
    pub fn enforce(mut self) -> Self {
        if !(self.show_session || self.show_weekly || self.show_fable) {
            self.show_session = true;
        }
        if !REFRESH_CHOICES.contains(&self.refresh_minutes) {
            self.refresh_minutes = 5;
        }
        self
    }

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
        assert!(d.show_session && d.show_weekly && d.show_fable && !d.show_session_reset);
        assert_eq!((d.style, d.amount, d.reset_format, d.refresh_minutes), (PanelStyle::Bars, Amount::Used, ResetFormat::Relative, 5));
        assert_eq!(d.clone().enforce(), d);
    }

    #[test]
    fn invariant() {
        let none = Config { show_session: false, show_weekly: false, show_fable: false, ..Config::default() };
        let fixed = none.enforce();
        assert!(fixed.show_session && !fixed.show_weekly && !fixed.show_fable);
        // Only the Fable window on is fine as it is.
        let fable = Config { show_session: false, show_weekly: false, ..Config::default() };
        assert_eq!(fable.clone().enforce(), fable);
    }

    #[test]
    fn refresh_interval() {
        for (stored, used) in [(1, 1), (5, 5), (15, 15), (0, 5), (2, 5), (60, 5)] {
            assert_eq!(Config { refresh_minutes: stored, ..Config::default() }.enforce().refresh_minutes, used);
        }
    }
}
