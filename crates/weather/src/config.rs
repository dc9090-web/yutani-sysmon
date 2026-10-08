//! Settings, stored with cosmic-config under the App ID (SPEC §11, version 1).
//! The API key is never here (SPEC §3).

use cosmic::cosmic_config::{self, CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry};
use serde::{Deserialize, Serialize};

pub const APP_ID: &str = "io.github.dc.CosmicAppletWeather";

/// At most this many saved cities.
pub const MAX_LOCATIONS: usize = 5;

/// The refresh intervals offered, in minutes.
pub const REFRESH_CHOICES: [u8; 3] = [15, 30, 60];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Location {
    /// A UUID, also the cache file name.
    pub id: String,
    pub name: String,
    /// `"<state>, <country>"` or `"<country>"`.
    pub region: String,
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Units {
    #[default]
    Metric,
    Imperial,
}

impl Units {
    /// The OpenWeather `units` parameter.
    pub fn param(self) -> &'static str {
        match self {
            Self::Metric => "metric",
            Self::Imperial => "imperial",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum IconSet {
    #[default]
    Detailed,
    System,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, CosmicConfigEntry)]
#[version = 1]
pub struct Config {
    pub locations: Vec<Location>,
    /// The panel city's id; `None` means the first location.
    pub panel_location: Option<String>,
    pub units: Units,
    pub icon_set: IconSet,
    pub show_city: bool,
    pub show_hilo: bool,
    pub refresh_minutes: u8,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            locations: Vec::new(),
            panel_location: None,
            units: Units::Metric,
            icon_set: IconSet::Detailed,
            show_city: false,
            show_hilo: false,
            refresh_minutes: 30,
        }
    }
}

impl Config {
    /// At most 5 locations, a panel city that exists, and a known interval
    /// (else 30 min).
    pub fn enforce(mut self) -> Self {
        self.locations.truncate(MAX_LOCATIONS);
        if self.panel_location.as_ref().is_some_and(|id| !self.locations.iter().any(|l| &l.id == id)) {
            self.panel_location = None;
        }
        if !REFRESH_CHOICES.contains(&self.refresh_minutes) {
            self.refresh_minutes = 30;
        }
        self
    }

    /// The city shown in the panel: the chosen one, else the first.
    pub fn panel(&self) -> Option<&Location> {
        self.panel_location.as_ref().and_then(|id| self.locations.iter().find(|l| &l.id == id)).or_else(|| self.locations.first())
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

    fn loc(id: &str) -> Location {
        Location { id: id.into(), name: id.into(), region: "AU".into(), lat: 0.0, lon: 0.0 }
    }

    #[test]
    fn defaults() {
        let d = Config::default();
        assert!(d.locations.is_empty() && d.panel_location.is_none());
        assert_eq!((d.units, d.icon_set, d.show_city, d.show_hilo, d.refresh_minutes), (Units::Metric, IconSet::Detailed, false, false, 30));
        assert_eq!(d.clone().enforce(), d);
        assert!(d.panel().is_none());
    }

    #[test]
    fn refresh_interval() {
        for (stored, used) in [(15, 15), (30, 30), (60, 60), (0, 30), (5, 30), (45, 30)] {
            assert_eq!(Config { refresh_minutes: stored, ..Config::default() }.enforce().refresh_minutes, used);
        }
    }

    #[test]
    fn locations() {
        let six: Vec<Location> = ["a", "b", "c", "d", "e", "f"].map(loc).into();
        let c = Config { locations: six, panel_location: Some("f".into()), ..Config::default() }.enforce();
        // Truncated to 5, so the panel city "f" is gone and falls back to the first.
        assert_eq!(c.locations.len(), 5);
        assert_eq!(c.panel_location, None);
        assert_eq!(c.panel().unwrap().id, "a");
        let c = Config { panel_location: Some("c".into()), ..c }.enforce();
        assert_eq!(c.panel().unwrap().id, "c");
    }
}
