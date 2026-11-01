//! Usage windows and the applet's state.

use chrono::{DateTime, TimeDelta, Utc};

use crate::api::Diagnostics;
use crate::auth::NoLogin;

/// The two windows, in their fixed display order. Weekly is the plan's
/// 7-day limit across all models, shown as "Session"; the 5-hour window
/// Claude used to return is ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Weekly,
    Fable,
}

impl Kind {
    pub const ALL: [Self; 2] = [Self::Weekly, Self::Fable];

    pub fn length(self) -> TimeDelta {
        TimeDelta::days(7)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Window {
    pub kind: Kind,
    /// Percent used, 0–100.
    pub used: f32,
    pub resets_at: Option<DateTime<Utc>>,
}

/// One parsed response: the windows present, in display order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Usage {
    pub windows: Vec<Window>,
}

impl Usage {
    pub fn get(&self, kind: Kind) -> Option<&Window> {
        self.windows.iter().find(|w| w.kind == kind)
    }
}

/// The last good response and when it arrived.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub usage: Usage,
    pub fetched_at: DateTime<Utc>,
}

/// What the applet is showing (SPEC §9). Limit reached and No Fable limit
/// are read off the snapshot rather than being states of their own.
#[derive(Debug, Clone, PartialEq)]
pub enum State {
    /// Started; nothing fetched yet.
    Loading,
    Normal,
    NotSignedIn(NoLogin),
    Expired,
    Offline,
    /// Paused until this time.
    RateLimited(DateTime<Utc>),
    Unrecognised(Diagnostics),
}

impl State {
    /// The short name used in debug logs.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Loading => "loading",
            Self::Normal => "normal",
            Self::NotSignedIn(_) => "not-signed-in",
            Self::Expired => "login-expired",
            Self::Offline => "offline",
            Self::RateLimited(_) => "rate-limited",
            Self::Unrecognised(_) => "format-not-recognised",
        }
    }
}
