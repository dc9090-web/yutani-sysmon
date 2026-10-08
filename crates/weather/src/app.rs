//! The applet: state, update, the panel button and the popup.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use chrono::{DateTime, TimeDelta, Utc};
use cosmic::app::{Core, Task};
use cosmic::cosmic_config;
use cosmic::iced::keyboard::{self, key::Named};
use cosmic::iced::window::Id;
use cosmic::iced::{Alignment, Length, Rectangle, Subscription};
use cosmic::surface::action::{app_popup, destroy_popup};
use cosmic::widget::segmented_button::{self, Entity, SingleSelectModel};
use cosmic::widget::{self, Column, Row};
use cosmic::{Element, applet, theme};
use i18n_embed::LanguageLoader;

use common::ink::Ink;
use common::panel::{Panel, Type};
use common::ui::{self, space_m, space_xs, space_xxs, space_xxxs};

use crate::api::{self, Error, Fetched, KeyCheck, Request, geocoding::Place};
use crate::cache;
use crate::config::{APP_ID, Config, IconSet, Location, MAX_LOCATIONS, REFRESH_CHOICES, TIME_APPLET, TimeConfig, Units};
use crate::fl;
use crate::format;
use crate::icons;
use crate::key::{self, Key, Storage};
use crate::model::{Snapshot, Tier};
use crate::scheduler::{self, Budget, Scheduler};
use crate::widgets::banner::banner;
use crate::widgets::header::{Header, header};
use crate::widgets::{Ctx, alert, current, daily, details, hourly, panel};

/// The applet-list icon, and the panel icon while there's nothing to show.
const DEFAULT_ICON: &str = "weather-few-clouds-symbolic";
/// Where the attribution links (SPEC §9.1, required by OpenWeather).
const ATTRIBUTION_URL: &str = "https://openweathermap.org";
/// Search waits this long after the last keystroke.
const SEARCH_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(400);
/// A result this close (degrees) to a saved city is the same city.
const SAME_PLACE: f64 = 0.05;
/// Room around the main page's scrolling middle: the panel, the header,
/// "Applet settings", the attribution and the popup padding. The middle
/// scrolls once the popup would pass the output or libcosmic's limit, so
/// the attribution always stays visible (DESIGN §6).
const MAIN_RESERVE: f32 = 260.0;

/// libcosmic's popup height limit.
const POPUP_MAX: f32 = 1000.0;
/// The settings header, the popup padding and the panel.
const SETTINGS_RESERVE: f32 = 120.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Main,
    Settings,
}

/// The settings page's key status line (SPEC §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyStatus {
    Unchecked,
    Checking,
    Checked(KeyCheck),
}

/// Why a refresh was asked for: the daily budget treats them differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// The panel city's interval.
    Scheduled,
    /// Popup open, switching the view, a units change, a new city.
    Auto,
    /// The refresh button.
    Manual,
}

/// One saved city's data and request state.
#[derive(Debug, Default)]
pub struct LocState {
    pub snapshot: Option<Snapshot>,
    pub fetching: bool,
    pub completed_at: Option<DateTime<Utc>>,
    /// The last request failed with a network or server error.
    pub offline: bool,
}

/// The search under "Add a city".
#[derive(Debug, Clone, Default, PartialEq)]
pub enum Search {
    #[default]
    Idle,
    Searching,
    Results(Vec<Place>),
    NoMatches,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seg {
    Units,
    Icons,
    Refresh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toggle {
    City,
    HiLo,
}

#[derive(Debug, Clone)]
pub enum Message {
    Surface(cosmic::surface::Action<Message>),
    TogglePopup(Rectangle, cosmic::iced::Vector),
    PopupClosed(Id),
    Config(Config),
    TimeConfig(TimeConfig),
    Page(Page),
    Escape,
    /// The 30 s clock: freshness text, catching up after suspend.
    Tick,
    /// A planned refresh is due, if the generation still matches.
    Wake(u64),
    CacheLoaded(cache::Loaded),
    KeyLoaded(Option<(Key, Storage)>),
    KeyInput(String),
    KeyVisibility,
    KeyTest,
    /// Saved (or not), checked, and the calls the check made.
    KeyTested(Key, Option<Storage>, KeyCheck, u32),
    KeyChecked(KeyCheck, u32),
    Fetched(String, Fetched),
    ManualRefresh,
    ToggleList,
    View(String),
    ToggleAlert(String),
    SetPanel(String),
    Remove(String),
    SearchInput(String),
    SearchFire(u64),
    Searched(u64, Result<Vec<Place>, Error>, u32),
    Add(usize),
    Segment(Seg, Entity),
    Show(Toggle, bool),
    OpenAttribution,
    /// An output's name and logical height.
    Output(Option<String>, Option<i32>),
    #[cfg(feature = "demo")]
    DemoNext,
    #[cfg(feature = "demo")]
    TakeShot,
    #[cfg(feature = "demo")]
    Shot(cosmic::iced::window::Screenshot),
}

pub struct App {
    core: Core,
    popup: Option<Id>,
    page: Page,
    config: Config,
    handler: Option<cosmic_config::Config>,
    client: reqwest::Client,
    military: bool,
    key: Option<Key>,
    storage: Option<Storage>,
    key_loaded: bool,
    key_input: String,
    key_hidden: bool,
    key_status: KeyStatus,
    /// The last known plan; an `Unknown` check keeps it.
    tier: Option<Tier>,
    /// 401 after a recheck: polling stops until the key changes.
    key_invalid: bool,
    places: HashMap<String, LocState>,
    cache_dir: Option<PathBuf>,
    cache_loaded: bool,
    budget: Budget,
    scheduler: Scheduler,
    /// Bumped whenever the planned refresh changes; stale wake-ups are ignored.
    timer: u64,
    now: DateTime<Utc>,
    /// The location shown in the popup.
    view: Option<String>,
    list_open: bool,
    open_alerts: HashSet<String>,
    search: String,
    search_gen: u64,
    search_abort: Option<cosmic::iced::task::Handle>,
    results: Search,
    already_saved: bool,
    units_model: SingleSelectModel,
    icons_model: SingleSelectModel,
    refresh_model: SingleSelectModel,
    output_height: Option<f32>,
    #[cfg(feature = "demo")]
    demo: Option<usize>,
}

impl App {
    fn interval(&self) -> TimeDelta {
        TimeDelta::minutes(i64::from(self.config.refresh_minutes))
    }

    fn lang() -> String {
        let l = crate::localize::LANGUAGE_LOADER.current_language();
        api::owm_lang(l.language.as_str(), l.region.as_ref().map(|r| r.as_str()), l.script.as_ref().map(|s| s.as_str()))
    }

    fn demo(&self) -> bool {
        #[cfg(feature = "demo")]
        return self.demo.is_some();
        #[cfg(not(feature = "demo"))]
        false
    }

    fn panel_id(&self) -> Option<String> {
        self.config.panel().map(|l| l.id.clone())
    }

    fn location(&self, id: &str) -> Option<&Location> {
        self.config.locations.iter().find(|l| l.id == id)
    }

    /// The snapshot for `id`, if it's in the current units.
    fn snapshot(&self, id: &str) -> Option<&Snapshot> {
        self.places.get(id).and_then(|p| p.snapshot.as_ref()).filter(|s| s.units == self.config.units)
    }

    fn age(&self, id: &str) -> Option<TimeDelta> {
        self.snapshot(id).map(|s| self.now - s.fetched_at)
    }

    fn fetching(&self, id: &str) -> bool {
        self.places.get(id).is_some_and(|p| p.fetching)
    }

    fn offline(&self, id: &str) -> bool {
        self.places.get(id).is_some_and(|p| p.offline)
    }

    /// Showing old values that can't be updated right now (SPEC §8).
    fn stale(&self, id: &str) -> bool {
        self.offline(id) || self.age(id).is_some_and(|a| a > self.interval() * 2)
    }

    // ---- refreshing -------------------------------------------------------------------

    /// Plans the next scheduled refresh. Replaces any earlier plan.
    fn arm(&mut self, due: Option<DateTime<Utc>>) -> Task<Message> {
        self.timer += 1;
        let Some(due) = due else {
            tracing::debug!("polling paused");
            return Task::none();
        };
        let wait = (due - Utc::now()).to_std().unwrap_or_default();
        tracing::debug!(at = %due, in_s = wait.as_secs(), "next refresh");
        let generation = self.timer;
        cosmic::task::future(async move {
            tokio::time::sleep(wait).await;
            Message::Wake(generation)
        })
    }

    /// A redraw when the refresh button's debounce or a pause ends.
    fn redraw_at(at: DateTime<Utc>) -> Task<Message> {
        let wait = (at - Utc::now()).to_std().unwrap_or_default();
        cosmic::task::future(async move {
            tokio::time::sleep(wait).await;
            Message::Tick
        })
    }

    /// The cache and key are both read: start polling the panel city.
    fn start(&mut self) -> Task<Message> {
        if !(self.cache_loaded && self.key_loaded) || self.key.is_none() || self.tier.is_none() || self.key_invalid {
            return Task::none();
        }
        let due = self.scheduler.plan(Utc::now());
        self.arm(Some(due))
    }

    /// Starts a refresh of `id` unless something stops it.
    fn refresh(&mut self, id: &str, why: Why) -> Task<Message> {
        if self.demo() || !self.cache_loaded || self.key_invalid {
            return Task::none();
        }
        let (Some(key), Some(loc)) = (self.key.clone(), self.location(id).cloned()) else { return Task::none() };
        let now = Utc::now();
        if self.fetching(id) || self.scheduler.paused(now) {
            return Task::none();
        }
        let allowed = match why {
            Why::Manual => self.budget.manual_ok(now),
            Why::Scheduled | Why::Auto => self.budget.automatic_ok(now),
        };
        if !allowed {
            tracing::debug!(calls = self.budget.today(now), ?why, "daily call limit reached");
            if why == Why::Scheduled {
                let at = Budget::resets_at(now) + TimeDelta::seconds((scheduler::jitter().abs() * 300.0) as i64);
                let due = self.scheduler.plan(at);
                return self.arm(Some(due));
            }
            return Task::none();
        }
        let Some(tier) = self.tier else {
            // The plan isn't known yet (offline at startup): check it first.
            if self.key_status != KeyStatus::Checking {
                return self.check_key(key);
            }
            return Task::none();
        };
        self.places.entry(id.to_owned()).or_default().fetching = true;
        tracing::debug!(location = %loc.name, ?why, ?tier, "refreshing");
        let req = Request { lat: loc.lat, lon: loc.lon, units: self.config.units, lang: Self::lang() };
        let client = self.client.clone();
        let id = id.to_owned();
        match tier {
            Tier::Full => {
                // Alerts already fetched are reused until they end.
                let known = self
                    .places
                    .get(&id)
                    .and_then(|p| p.snapshot.as_ref())
                    .map(|s| s.alerts.iter().filter(|a| a.end > now.timestamp()).cloned().collect())
                    .unwrap_or_default();
                cosmic::task::future(async move { Message::Fetched(id, api::fetch_full(client, key, req, known).await) })
            }
            Tier::Free => cosmic::task::future(async move { Message::Fetched(id, api::fetch_free(client, key, req).await) }),
        }
    }

    /// Adds calls to the day's count and saves it.
    fn count(&mut self, calls: u32) -> Task<Message> {
        if calls == 0 {
            return Task::none();
        }
        let now = Utc::now();
        self.budget.add(calls, now);
        tracing::debug!(calls_today = self.budget.today(now), "call counter");
        let Some(dir) = self.cache_dir.clone() else { return Task::none() };
        let budget = self.budget;
        background(async move { cache::save_budget(dir, budget).await })
    }

    fn fetched(&mut self, id: String, f: Fetched) -> Task<Message> {
        let now = Utc::now();
        self.now = now;
        let mut tasks = vec![self.count(f.calls)];
        let is_panel = self.panel_id().as_deref() == Some(id.as_str());
        let units = self.config.units;
        let place = self.places.entry(id.clone()).or_default();
        place.fetching = false;
        place.completed_at = Some(now);
        let timing = match f.result {
            Ok(s) => {
                place.offline = false;
                // A units change while this was in flight: drop the old units.
                if s.units == units {
                    if let Some(dir) = self.cache_dir.clone() {
                        let (id, s) = (id.clone(), s.clone());
                        tasks.push(background(async move { cache::save(dir, id, s).await }));
                    }
                    place.snapshot = Some(s);
                }
                scheduler::Result::Ok
            }
            Err(Error::Unauthorized) => {
                // The key may have lost its subscription: check it once more.
                tracing::debug!("401: checking the key again");
                if let Some(k) = self.key.clone() {
                    tasks.push(self.check_key(k));
                }
                return Task::batch(tasks);
            }
            Err(Error::RateLimited(wait)) => {
                tracing::debug!(wait_s = wait.as_secs(), "rate limited");
                self.scheduler.paused_until = Some(now + TimeDelta::from_std(wait).unwrap_or(TimeDelta::minutes(10)));
                scheduler::Result::RateLimited(wait)
            }
            Err(e) => {
                tracing::debug!(?e, "refresh failed");
                place.offline = true;
                scheduler::Result::Offline
            }
        };
        if is_panel {
            let due = self.scheduler.completed(timing, now, self.interval(), scheduler::jitter());
            tasks.push(self.arm(due));
        }
        tasks.push(Self::redraw_at(now + scheduler::DEBOUNCE));
        if let Some(p) = self.scheduler.paused_until.filter(|p| *p > now) {
            tasks.push(Self::redraw_at(p));
        }
        Task::batch(tasks)
    }

    fn check_key(&mut self, key: Key) -> Task<Message> {
        self.key_status = KeyStatus::Checking;
        cosmic::task::future(api::detect_tier(self.client.clone(), key)).map(|(check, calls)| cosmic::Action::App(Message::KeyChecked(check, calls)))
    }

    /// A key check finished: (re)start polling, or stop it.
    fn checked(&mut self, check: KeyCheck) -> Task<Message> {
        self.key_status = KeyStatus::Checked(check);
        let now = Utc::now();
        match check {
            KeyCheck::Works(t) => {
                tracing::debug!(tier = ?t, "plan");
                self.tier = Some(t);
                self.key_invalid = false;
                self.start()
            }
            KeyCheck::Invalid => {
                tracing::debug!("key not accepted; polling stops until it changes");
                self.tier = None;
                self.key_invalid = true;
                let due = self.scheduler.completed(scheduler::Result::Halt, now, self.interval(), 0.0);
                self.arm(due)
            }
            KeyCheck::Unknown => {
                // Offline: try again through the normal backoff.
                let due = self.scheduler.completed(scheduler::Result::Offline, now, self.interval(), 0.0);
                self.arm(due)
            }
        }
    }

    /// Refetches `id` if it has no data or data older than `max_age`.
    fn refresh_if_older(&mut self, id: &str, max_age: TimeDelta) -> Task<Message> {
        if self.age(id).is_none_or(|a| a > max_age) { self.refresh(id, Why::Auto) } else { Task::none() }
    }

    /// The refresh button: not in flight, 30 s after the last request, no
    /// 429 pause, under the daily maximum.
    fn can_refresh(&self, id: &str) -> bool {
        let now = Utc::now();
        !self.demo()
            && self.key.is_some()
            && !self.key_invalid
            && !self.fetching(id)
            && self.places.get(id).and_then(|p| p.completed_at).is_none_or(|t| now - t >= scheduler::DEBOUNCE)
            && !self.scheduler.paused(now)
            && self.budget.manual_ok(now)
    }

    // ---- config -----------------------------------------------------------------------

    /// Saves one setting, then applies the change.
    fn save(&mut self, set: impl FnOnce(&mut Config, &cosmic_config::Config) -> Result<bool, cosmic_config::Error>) -> Task<Message> {
        let mut c = self.config.clone();
        match &self.handler {
            Some(h) => {
                if let Err(e) = set(&mut c, h) {
                    tracing::error!("saving config: {e}");
                }
            }
            None => tracing::error!("no config handler; setting not saved"),
        }
        self.apply(c)
    }

    /// Applies a new config, from our own saves or from outside.
    fn apply(&mut self, c: Config) -> Task<Message> {
        let c = c.enforce();
        if c == self.config {
            return Task::none();
        }
        let old = std::mem::replace(&mut self.config, c);
        tracing::debug!(locations = self.config.locations.len(), units = ?self.config.units, "config changed");
        self.sync_models();
        let mut tasks = Vec::new();
        // Forget deleted cities.
        let ids: HashSet<String> = self.config.locations.iter().map(|l| l.id.clone()).collect();
        let gone: Vec<String> = self.places.keys().filter(|k| !ids.contains(*k)).cloned().collect();
        for id in gone {
            self.places.remove(&id);
            if let Some(dir) = self.cache_dir.clone() {
                tasks.push(background(async move { cache::remove(dir, id).await }));
            }
        }
        if self.view.as_ref().is_none_or(|v| !ids.contains(v)) {
            self.view = self.panel_id();
        }
        let panel = self.panel_id();
        if self.config.units != old.units {
            // Changing units refetches at once.
            let mut targets: Vec<String> = panel.iter().chain(self.view.iter()).cloned().collect();
            targets.dedup();
            for id in targets {
                tasks.push(self.refresh(&id, Why::Auto));
            }
        } else if panel != old.panel().map(|l| l.id.clone())
            && let Some(id) = panel
        {
            tasks.push(self.refresh_if_older(&id, self.interval()));
        }
        if self.config.refresh_minutes != old.refresh_minutes {
            let due = self.scheduler.retime(Utc::now(), self.interval(), scheduler::jitter());
            tasks.push(self.arm(due));
        }
        Task::batch(tasks)
    }

    fn sync_models(&mut self) {
        fn select<T: PartialEq + 'static>(m: &mut SingleSelectModel, want: &T) {
            let found = m.iter().find(|e| m.data::<T>(*e) == Some(want));
            if let Some(e) = found {
                m.activate(e);
            }
        }
        select(&mut self.units_model, &self.config.units);
        select(&mut self.icons_model, &self.config.icon_set);
        select(&mut self.refresh_model, &self.config.refresh_minutes);
    }

    fn segment(&mut self, seg: Seg, e: Entity) -> Task<Message> {
        match seg {
            Seg::Units => match self.units_model.data::<Units>(e).copied() {
                Some(v) => self.save(|c, h| c.set_units(h, v)),
                None => Task::none(),
            },
            Seg::Icons => match self.icons_model.data::<IconSet>(e).copied() {
                Some(v) => self.save(|c, h| c.set_icon_set(h, v)),
                None => Task::none(),
            },
            Seg::Refresh => match self.refresh_model.data::<u8>(e).copied() {
                Some(v) => self.save(|c, h| c.set_refresh_minutes(h, v)),
                None => Task::none(),
            },
        }
    }

    /// Saves a new location list and panel city.
    fn save_locations(&mut self, locations: Vec<Location>, panel: Option<String>) -> Task<Message> {
        self.save(move |c, h| {
            c.set_locations(h, locations)?;
            c.set_panel_location(h, panel)
        })
    }

    // ---- search -----------------------------------------------------------------------

    fn search_input(&mut self, s: String) -> Task<Message> {
        self.search = s;
        self.already_saved = false;
        self.search_gen += 1;
        // A new query cancels the request in flight.
        if let Some(h) = self.search_abort.take() {
            h.abort();
        }
        if self.search.trim().chars().count() < 2 {
            self.results = Search::Idle;
            return Task::none();
        }
        let generation = self.search_gen;
        cosmic::task::future(async move {
            tokio::time::sleep(SEARCH_DEBOUNCE).await;
            Message::SearchFire(generation)
        })
    }

    fn search_fire(&mut self, generation: u64) -> Task<Message> {
        if generation != self.search_gen || self.demo() {
            return Task::none();
        }
        let Some(key) = self.key.clone() else {
            self.results = Search::Unavailable;
            return Task::none();
        };
        self.results = Search::Searching;
        let (task, handle) = cosmic::task::future(api::search(self.client.clone(), key, self.search.trim().to_owned(), Self::lang()))
            .map(move |(r, calls)| cosmic::Action::App(Message::Searched(generation, r, calls)))
            .abortable();
        self.search_abort = Some(handle);
        task
    }

    fn add(&mut self, i: usize) -> Task<Message> {
        let Search::Results(results) = &self.results else { return Task::none() };
        let Some(p) = results.get(i).cloned() else { return Task::none() };
        if self.config.locations.iter().any(|l| (l.lat - p.lat).abs() < SAME_PLACE && (l.lon - p.lon).abs() < SAME_PLACE) {
            self.already_saved = true;
            return Task::none();
        }
        if self.config.locations.len() >= MAX_LOCATIONS {
            return Task::none();
        }
        let loc = Location { id: uuid::Uuid::new_v4().to_string(), name: p.name, region: p.region, lat: p.lat, lon: p.lon };
        let id = loc.id.clone();
        let first = self.config.locations.is_empty();
        let mut locations = self.config.locations.clone();
        locations.push(loc);
        self.search.clear();
        self.results = Search::Idle;
        let panel = self.config.panel_location.clone();
        let saved = self.save_locations(locations, panel);
        if first {
            self.view = Some(id.clone());
        }
        // Fetch it now so its row shows a temperature.
        Task::batch([saved, self.refresh(&id, Why::Auto)])
    }

    // ---- panel ------------------------------------------------------------------------

    /// Why the panel has no temperature, if it has none.
    fn idle_reason(&self) -> Option<String> {
        if self.key_loaded && self.key.is_none() {
            Some(fl!("no-key-title"))
        } else if self.key_invalid {
            Some(fl!("invalid-key-title"))
        } else if self.config.locations.is_empty() {
            Some(fl!("no-locations-title"))
        } else {
            None
        }
    }

    fn panel_content(&self) -> panel::Content {
        let idle = panel::Content { icon: default_icon(), temp: None, city: None, hilo: None, alert: false, stale: false };
        if self.idle_reason().is_some() {
            return idle;
        }
        let Some(loc) = self.config.panel() else { return idle };
        let Some(s) = self.snapshot(&loc.id) else { return idle };
        let today = s.daily.first();
        panel::Content {
            icon: condition_icon(self.config.icon_set, s),
            temp: Some(s.current.temp),
            city: self.config.show_city.then(|| loc.name.clone()),
            hilo: if self.config.show_hilo { today.map(|d| (d.max, d.min)) } else { None },
            alert: !s.alerts.is_empty(),
            stale: self.stale(&loc.id),
        }
    }

    /// "Sydney: 23°, Partly cloudy · H 27° L 15° · Severe Thunderstorm Warning".
    fn tooltip(&self) -> String {
        if let Some(r) = self.idle_reason() {
            return r;
        }
        let Some(loc) = self.config.panel() else { return fl!("applet-name") };
        let Some(s) = self.snapshot(&loc.id) else { return loc.name.clone() };
        let today = s.daily.first();
        let mut text = fl!(
            "a11y-panel",
            city = loc.name.clone(),
            temp = format::temp(Some(s.current.temp)),
            desc = format::sentence(&s.current.condition.description),
            high = format::temp(today.map(|d| d.max)),
            low = format::temp(today.map(|d| d.min))
        );
        if let Some(a) = s.alerts.first() {
            text.push_str(" · ");
            text.push_str(&a.event);
        }
        text
    }

    fn panel_view(&self) -> Element<'_, Message> {
        let p = Panel::of(&self.core.applet);
        let (major, minor) = self.core.applet.suggested_padding(true);
        let (iw, ih) = p.icon;
        let thickness = if p.horizontal { ih } else { iw } + 2.0 * f32::from(minor);
        let content = panel::content(self.panel_content(), &p);
        let (pad, w, h) =
            if p.horizontal { ([0, major], Length::Shrink, Length::Fixed(thickness)) } else { ([major, 0], Length::Fixed(thickness), Length::Shrink) };
        let button = widget::button::custom(widget::container(content).center_x(w).center_y(h))
            .padding(pad)
            .class(theme::Button::AppletIcon)
            .on_press_with_rectangle(|offset, bounds| Message::TogglePopup(bounds, offset));
        let tip = self.core.applet.applet_tooltip::<Message>(button, self.tooltip(), self.popup.is_some(), Message::Surface, None);
        self.core.applet.autosize_window(tip).into()
    }

    // ---- popup ------------------------------------------------------------------------

    fn popup_view(&self) -> Element<'_, Message> {
        let content = match self.page {
            Page::Main => self.main_page(),
            Page::Settings => self.settings_page(),
        };
        widget::container(content).padding([8, 0, 8, 0]).into()
    }

    fn ctx(&self, id: &str) -> Ctx {
        Ctx { units: self.config.units, icons: self.config.icon_set, military: self.military, dim: self.offline(id) }
    }

    /// The header's freshness text and whether it's a warning.
    fn status(&self, id: &str) -> Option<(String, bool)> {
        let now = self.now;
        if let Some(until) = self.scheduler.paused_until.filter(|p| *p > now) {
            return Some((fl!("rate-limited", time = format::countdown(until, now)), true));
        }
        let ago = self.age(id).and_then(format::ago);
        if self.offline(id) {
            return Some((fl!("offline-ago", time = ago.unwrap_or_else(|| "0m".into())), true));
        }
        if self.fetching(id) && self.snapshot(id).is_none() {
            return Some((fl!("refreshing"), false));
        }
        self.snapshot(id)?;
        Some((ago.map_or_else(|| fl!("updated-just-now"), |t| fl!("updated-ago", time = t)), false))
    }

    fn header(&self, id: &str) -> Element<'_, Message> {
        let loc = self.location(id);
        header(Header {
            city: loc.map_or_else(|| fl!("applet-name"), |l| l.name.clone()),
            region: loc.map(|l| l.region.clone()).unwrap_or_default(),
            switch: (self.config.locations.len() > 1).then_some(Message::ToggleList),
            status: if self.key_invalid { None } else { self.status(id) },
            refresh: (!self.key_invalid).then(|| self.can_refresh(id).then_some(Message::ManualRefresh)),
            fetching: self.fetching(id),
        })
    }

    fn location_icon(&self, id: &str) -> widget::icon::Handle {
        self.snapshot(id).map_or_else(default_icon, |s| condition_icon(self.config.icon_set, s))
    }

    /// The saved cities under the header, to switch the popup's view.
    fn location_list(&self) -> Element<'_, Message> {
        let panel = self.panel_id();
        let mut col = Column::new();
        for l in &self.config.locations {
            let name = if panel.as_deref() == Some(l.id.as_str()) { format!("{} · {}", l.name, fl!("panel-tag")) } else { l.name.clone() };
            let row = Row::new()
                .spacing(space_xs())
                .align_y(Alignment::Center)
                .push(widget::icon(self.location_icon(&l.id)).size(16))
                .push(widget::text::body(name).width(Length::Fill))
                .push(ui::mono(
                    format::temp(self.snapshot(&l.id).map(|s| s.current.temp)),
                    Type::new(14.0, 20.0),
                    cosmic::iced::font::Weight::Normal,
                    Some(Ink::Muted),
                ));
            col = col.push(applet::menu_button(row).on_press(Message::View(l.id.clone())));
        }
        col.into()
    }

    /// "Daily call limit reached · resumes 10:00", when the counter is at the cap.
    fn daily_limit(&self) -> Option<Element<'_, Message>> {
        if self.budget.automatic_ok(self.now) {
            return None;
        }
        // 00:00 UTC on this machine's clock.
        let at = Budget::resets_at(self.now).with_timezone(&chrono::Local);
        let time = if self.military { at.format("%H:%M").to_string() } else { at.format("%-I:%M %p").to_string() };
        Some(caption(fl!("daily-limit", time = time), Ink::Warning))
    }

    fn main_page(&self) -> Element<'_, Message> {
        let id = self.view.clone().or_else(|| self.panel_id());
        let gate = if self.key_loaded && self.key.is_none() {
            Some(banner(fl!("no-key-title"), fl!("no-key-body"), Some((fl!("open-settings"), Message::Page(Page::Settings)))))
        } else if self.key_invalid {
            Some(banner(fl!("invalid-key-title"), fl!("invalid-key-body"), Some((fl!("open-settings"), Message::Page(Page::Settings)))))
        } else if self.config.locations.is_empty() {
            Some(banner(fl!("no-locations-title"), fl!("no-locations-body"), Some((fl!("add-location"), Message::Page(Page::Settings)))))
        } else {
            None
        };
        let mut col = Column::new();
        match id.as_deref().filter(|_| self.key.is_some()) {
            Some(id) => {
                col = col.push(self.header(id));
                if self.list_open {
                    col = col.push(self.location_list());
                }
            }
            None => col = col.push(widget::container(widget::text::heading(fl!("applet-name"))).padding([space_xxs(), space_m()])),
        }
        col = match gate {
            Some(g) => col.push(g),
            None => col.push_maybe(self.daily_limit()),
        };
        let body = id.as_deref().filter(|_| !self.key_invalid && self.key.is_some()).and_then(|id| self.forecast(id));
        if let Some(body) = body {
            let limit = (self.output_height.unwrap_or(POPUP_MAX).min(POPUP_MAX) - MAIN_RESERVE).max(240.0);
            col = col.push(widget::container(widget::scrollable(body)).max_height(limit));
        }
        col.push(ui::inset_divider()).push(ui::settings_row(fl!("applet-settings"), Message::Page(Page::Settings))).push(self.attribution()).into()
    }

    /// Everything between the header and "Applet settings".
    fn forecast(&self, id: &str) -> Option<Element<'_, Message>> {
        let s = self.snapshot(id)?;
        let ctx = self.ctx(id);
        let free = s.tier == Tier::Free;
        let mut col = Column::new().push(current::current(s, ctx));
        let lang = Self::lang();
        for a in &s.alerts {
            col = col.push(alert::alert(a, s.tz_offset, self.military, &lang, self.open_alerts.contains(&a.id), Message::ToggleAlert(a.id.clone())));
        }
        if !s.hourly.is_empty() {
            let h = hourly::Hourly { hours: &s.hourly, tz: s.tz_offset, military: self.military, dim: ctx.dim, every_point: free };
            // Without AT-SPI, the canvas's text summary is its tooltip.
            let summary = h.summary().unwrap_or_default();
            let well = ui::well(widget::container(hourly::view(h)).padding(space_xs()));
            col = col.push(ui::inset_divider()).push(ui::group_label(if free { fl!("next-24h-free") } else { fl!("next-24h") })).push(
                widget::container(widget::tooltip(well, widget::text::caption(summary), widget::tooltip::Position::Top)).padding([space_xxs(), space_m()]),
            );
        }
        if !s.daily.is_empty() {
            col = col.push(ui::inset_divider()).push(ui::group_label(if free { fl!("forecast-5") } else { fl!("forecast-7") })).push(daily::daily(s, ctx));
        }
        col = col.push(ui::inset_divider()).push(details::details(s, ctx));
        if free {
            col = col.push(caption(fl!("free-plan-note"), Ink::Muted));
        }
        Some(col.into())
    }

    /// "Weather data: OpenWeather", linking to openweathermap.org. Always shown.
    fn attribution(&self) -> Element<'_, Message> {
        let link = widget::button::custom(widget::text::caption(fl!("attribution")).class(Ink::Muted.text()))
            .class(theme::Button::Link)
            .padding(0)
            .on_press(Message::OpenAttribution);
        widget::container(link).padding([space_xxs(), space_m()]).into()
    }

    fn settings_page(&self) -> Element<'_, Message> {
        fn seg(model: &SingleSelectModel, s: Seg) -> Element<'_, Message> {
            applet::padded_control(
                widget::segmented_control::horizontal(model).button_padding([space_xxxs(), 0, space_xxxs(), 0]).on_activate(move |e| Message::Segment(s, e)),
            )
            .into()
        }
        let toggle = |t: Toggle, title: String, desc: String, on: bool| {
            applet::padded_control(
                Row::new()
                    .spacing(space_xs())
                    .align_y(Alignment::Center)
                    .push(Column::new().width(Length::Fill).push(widget::text::body(title)).push(widget::text::caption(desc).class(Ink::Muted.text())))
                    .push(widget::toggler(on).on_toggle(move |v| Message::Show(t, v))),
            )
        };
        let icons_note = match self.config.icon_set {
            IconSet::Detailed => fl!("icons-detailed-note"),
            IconSet::System => fl!("icons-system-note"),
        };
        let body = Column::new()
            .push(ui::group_label(fl!("locations")))
            .push(self.locations_section())
            .push(ui::inset_divider())
            .push(ui::group_label(fl!("weather-icons")))
            .push(seg(&self.icons_model, Seg::Icons))
            .push(caption(icons_note, Ink::Muted))
            .push(ui::inset_divider())
            .push(ui::group_label(fl!("units")))
            .push(seg(&self.units_model, Seg::Units))
            .push(ui::inset_divider())
            .push(ui::group_label(fl!("panel")))
            .push(toggle(Toggle::City, fl!("show-city"), fl!("show-city-desc"), self.config.show_city))
            .push(toggle(Toggle::HiLo, fl!("show-hilo"), fl!("show-hilo-desc"), self.config.show_hilo))
            .push(ui::inset_divider())
            .push(ui::group_label(fl!("refresh-every")))
            .push(seg(&self.refresh_model, Seg::Refresh))
            .push(ui::inset_divider())
            .push(ui::group_label(fl!("api-key")))
            .push(self.key_section());
        // The popup is at most 1000 px tall, and the output may be shorter.
        let limit = (self.output_height.unwrap_or(POPUP_MAX).min(POPUP_MAX) - SETTINGS_RESERVE).max(240.0);
        Column::new()
            .push(ui::back_header(fl!("applet-settings"), fl!("back"), Message::Page(Page::Main)))
            .push(widget::container(widget::scrollable(body)).max_height(limit))
            .into()
    }

    fn locations_section(&self) -> Element<'_, Message> {
        let panel = self.panel_id();
        let last = self.config.locations.len() <= 1;
        let mut col = Column::new();
        for l in &self.config.locations {
            let selected = panel.as_deref() == Some(l.id.as_str());
            let region = if selected { format!("{} · {}", l.region, fl!("in-panel")) } else { l.region.clone() };
            let choose = applet::menu_button(
                Row::new()
                    .spacing(space_xs())
                    .align_y(Alignment::Center)
                    .push(ui::radio_dot(selected))
                    .push(widget::icon(self.location_icon(&l.id)).size(16))
                    .push(
                        Column::new().width(Length::Fill).push(widget::text::body(l.name.clone())).push(widget::text::caption(region).class(Ink::Muted.text())),
                    )
                    .push(ui::mono(
                        format::temp(self.snapshot(&l.id).map(|s| s.current.temp)),
                        Type::new(14.0, 20.0),
                        cosmic::iced::font::Weight::Normal,
                        Some(Ink::Muted),
                    )),
            )
            .on_press(Message::SetPanel(l.id.clone()));
            let remove = widget::button::icon(widget::icon::from_name("edit-delete-symbolic").size(16))
                .on_press_maybe((!last).then(|| Message::Remove(l.id.clone())))
                .tooltip(fl!("remove-location", name = l.name.clone()));
            col =
                col.push(Row::new().align_y(Alignment::Center).push(widget::container(choose).width(Length::Fill)).push(widget::container(remove).padding([
                    0,
                    space_xs(),
                    0,
                    0,
                ])));
        }
        let n = self.config.locations.len();
        if n >= MAX_LOCATIONS {
            return col.push(caption(fl!("saved-full"), Ink::Muted)).into();
        }
        if n > 0 {
            col = col.push(caption(fl!("saved-count", n = n), Ink::Muted));
        }
        let input = widget::search_input(fl!("add-city"), &self.search).on_input(Message::SearchInput).on_clear(Message::SearchInput(String::new()));
        col = col.push(applet::padded_control(input));
        if self.already_saved {
            col = col.push(caption(fl!("already-saved"), Ink::Warning));
        }
        match &self.results {
            Search::Idle | Search::Searching => {}
            Search::NoMatches => col = col.push(caption(fl!("no-matches"), Ink::Muted)),
            Search::Unavailable => col = col.push(caption(fl!("search-unavailable"), Ink::Warning)),
            Search::Results(places) => {
                for (i, p) in places.iter().enumerate() {
                    let sub = format!("{} · {:.2}, {:.2}", p.region, p.lat, p.lon);
                    let row = Row::new()
                        .spacing(space_xs())
                        .align_y(Alignment::Center)
                        .push(widget::icon::from_name("list-add-symbolic").size(16).symbolic(true))
                        .push(
                            Column::new()
                                .width(Length::Fill)
                                .push(widget::text::body(p.name.clone()))
                                .push(widget::text::caption(sub).class(Ink::Muted.text())),
                        );
                    col = col.push(applet::menu_button(row).on_press(Message::Add(i)));
                }
            }
        }
        col.into()
    }

    fn key_section(&self) -> Element<'_, Message> {
        let placeholder = if self.key.is_some() { fl!("key-placeholder-saved") } else { fl!("key-placeholder") };
        let typed = Key::new(&self.key_input);
        let checking = self.key_status == KeyStatus::Checking;
        let input = widget::secure_input(placeholder, &self.key_input, Some(Message::KeyVisibility), self.key_hidden)
            .on_input(Message::KeyInput)
            .on_submit_maybe(typed.is_some().then_some(|_| Message::KeyTest))
            .width(Length::Fill);
        let test = widget::button::standard(fl!("test"))
            .on_press_maybe((!checking && !self.demo() && (typed.is_some() || self.key.is_some())).then_some(Message::KeyTest));
        let mut col = Column::new().spacing(space_xxs()).push(Row::new().spacing(space_xs()).align_y(Alignment::Center).push(input).push(test));
        if typed.is_some() && !key::looks_valid(&self.key_input) {
            col = col.push(widget::text::caption(fl!("key-shape")).class(Ink::Warning.text()));
        }
        let status = match self.key_status {
            KeyStatus::Unchecked => None,
            KeyStatus::Checking => Some((fl!("checking"), Ink::Muted)),
            KeyStatus::Checked(KeyCheck::Works(Tier::Full)) => Some((fl!("key-full"), Ink::Success)),
            KeyStatus::Checked(KeyCheck::Works(Tier::Free)) => Some((fl!("key-free"), Ink::Warning)),
            KeyStatus::Checked(KeyCheck::Invalid) => Some((fl!("key-invalid"), Ink::Destructive)),
            KeyStatus::Checked(KeyCheck::Unknown) => Some((fl!("key-offline"), Ink::Muted)),
        };
        if let Some((text, ink)) = status {
            col = col.push(widget::text::caption(text).class(ink.text()));
        }
        if self.key_status == KeyStatus::Checked(KeyCheck::Invalid) {
            col = col.push(widget::text::caption(fl!("invalid-key-body")).class(Ink::Muted.text()));
        }
        if self.storage == Some(Storage::File) {
            col = col.push(widget::text::caption(fl!("key-stored-file")).class(Ink::Muted.text()));
        }
        col = col.push(widget::text::caption(fl!("key-limit-tip")).class(Ink::Muted.text()));
        applet::padded_control(col).into()
    }

    // ---- demo -------------------------------------------------------------------------

    #[cfg(feature = "demo")]
    fn show_scene(&mut self, i: usize) {
        let s = crate::demo::scene(i, Utc::now());
        tracing::debug!(scene = i, name = s.name, "demo");
        self.demo = Some(i);
        self.now = Utc::now();
        self.key_loaded = true;
        self.key = s.key;
        self.key_invalid = s.key_invalid;
        self.key_status = s.key_status;
        self.tier = s.tier;
        self.config.locations = s.locations;
        self.config.panel_location = None;
        self.config.units = s.units;
        self.config.icon_set = s.icons;
        self.config.show_city = s.show_city;
        self.config.show_hilo = s.show_hilo;
        self.places = s.places;
        self.view = self.panel_id();
        self.list_open = s.list_open;
        self.budget = s.budget;
        self.scheduler = Scheduler::default();
        self.scheduler.paused_until = s.paused_until;
        self.page = s.page;
        self.search = s.search;
        self.results = s.results;
        self.open_alerts = s.open_alerts;
        self.sync_models();
    }

    /// Demo mode shows the panel button's content above the popup.
    #[cfg(feature = "demo")]
    fn panel_preview(&self) -> Element<'_, Message> {
        let p = Panel::of(&self.core.applet);
        let content = panel::content(self.panel_content(), &p);
        widget::container(widget::container(content).padding([4, 12]).class(theme::Container::Card)).padding([8, 24]).into()
    }
}

/// Runs file I/O off the UI thread, with no message back.
fn background(f: impl std::future::Future<Output = ()> + Send + 'static) -> Task<Message> {
    cosmic::iced::Task::future(f).discard()
}

fn default_icon() -> widget::icon::Handle {
    widget::icon::from_name(DEFAULT_ICON).symbolic(true).handle()
}

/// The snapshot's current condition in the chosen set.
fn condition_icon(set: IconSet, s: &Snapshot) -> widget::icon::Handle {
    icons::condition(set, &s.current.condition, Some(format::wind_ms(s.current.wind_speed, s.units)))
}

/// A caption line with the popup's side padding.
fn caption<'a, M: 'a>(text: String, ink: Ink) -> Element<'a, M> {
    widget::container(widget::text::caption(text).class(ink.text())).padding([space_xxs(), space_m()]).width(Length::Fill).into()
}

impl cosmic::Application for App {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, _: ()) -> (Self, Task<Message>) {
        let (handler, config) = Config::load();
        tracing::debug!(locations = config.locations.len(), units = ?config.units, icons = ?config.icon_set, refresh = config.refresh_minutes, "config loaded");
        let now = Utc::now();
        let mut app = Self {
            core,
            popup: None,
            page: if crate::preview_settings() { Page::Settings } else { Page::Main },
            view: config.panel().map(|l| l.id.clone()),
            config,
            handler,
            client: api::client(),
            military: TimeConfig::load().military_time,
            key: None,
            storage: None,
            key_loaded: false,
            key_input: String::new(),
            key_hidden: true,
            key_status: KeyStatus::Unchecked,
            tier: None,
            key_invalid: false,
            places: HashMap::new(),
            cache_dir: cache::dir(),
            cache_loaded: false,
            budget: Budget::new(now),
            scheduler: Scheduler::default(),
            timer: 0,
            now,
            list_open: false,
            open_alerts: HashSet::new(),
            search: String::new(),
            search_gen: 0,
            search_abort: None,
            results: Search::Idle,
            already_saved: false,
            units_model: segmented_button::Model::builder()
                .insert(|b| b.text(fl!("units-metric")).data(Units::Metric))
                .insert(|b| b.text(fl!("units-imperial")).data(Units::Imperial))
                .build(),
            icons_model: segmented_button::Model::builder()
                .insert(|b| b.text(fl!("icons-detailed")).data(IconSet::Detailed))
                .insert(|b| b.text(fl!("icons-system")).data(IconSet::System))
                .build(),
            refresh_model: {
                let mut m = segmented_button::Model::builder().build();
                for n in REFRESH_CHOICES {
                    m.insert().text(fl!("minutes", n = n)).data(n);
                }
                m
            },
            output_height: None,
            #[cfg(feature = "demo")]
            demo: None,
        };
        app.sync_models();
        #[cfg(feature = "demo")]
        if crate::demo::enabled() {
            app.cache_loaded = true;
            app.show_scene(crate::demo::first());
            if crate::demo::shot_path().is_some() {
                return (
                    app,
                    cosmic::task::future(async {
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        Message::TakeShot
                    }),
                );
            }
            return (app, Task::none());
        }
        let ids = app.config.locations.iter().map(|l| l.id.clone()).collect();
        let load_cache = match app.cache_dir.clone() {
            Some(dir) => cosmic::task::future(async move { Message::CacheLoaded(cache::load(dir, ids, now).await) }),
            None => cosmic::task::message(Message::CacheLoaded(cache::Loaded::default())),
        };
        let load_key = cosmic::task::future(key::load()).map(|k| cosmic::Action::App(Message::KeyLoaded(k)));
        // `WEATHER_SHOT` with live data: capture once the first refresh is in.
        #[cfg(feature = "demo")]
        if crate::demo::shot_path().is_some() {
            let shot = cosmic::task::future(async {
                tokio::time::sleep(std::time::Duration::from_secs(8)).await;
                Message::TakeShot
            });
            return (app, Task::batch([load_cache, load_key, shot]));
        }
        (app, Task::batch([load_cache, load_key]))
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn subscription(&self) -> Subscription<Message> {
        #[cfg_attr(not(feature = "demo"), expect(unused_mut))]
        let mut subs = vec![
            cosmic::iced::time::every(std::time::Duration::from_secs(30)).map(|_| Message::Tick),
            self.core.watch_config::<Config>(APP_ID).map(|u| Message::Config(u.config)),
            self.core.watch_config::<TimeConfig>(TIME_APPLET).map(|u| Message::TimeConfig(u.config)),
            events(),
        ];
        #[cfg(feature = "demo")]
        if self.demo.is_some() && crate::demo::shot_path().is_none() {
            subs.push(cosmic::iced::time::every(crate::demo::STEP).map(|_| Message::DemoNext));
        }
        Subscription::batch(subs)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Surface(a) => return cosmic::task::message(cosmic::Action::Surface(a)),
            Message::TogglePopup(bounds, offset) => {
                if let Some(id) = self.popup.take() {
                    return cosmic::task::message(cosmic::Action::Surface(destroy_popup(id)));
                }
                // The popup always opens on Main, showing the panel city.
                self.page = Page::Main;
                self.list_open = false;
                self.view = self.panel_id();
                self.now = Utc::now();
                let open = cosmic::task::message(cosmic::Action::Surface(open_popup(bounds, offset)));
                let Some(id) = self.view.clone() else { return open };
                return Task::batch([open, self.refresh_if_older(&id, scheduler::STALE_ON_OPEN)]);
            }
            Message::PopupClosed(id) => {
                if self.popup == Some(id) {
                    self.popup = None;
                }
            }
            Message::Config(c) => {
                if !self.demo() {
                    return self.apply(c);
                }
            }
            Message::TimeConfig(t) => self.military = t.military_time,
            Message::Page(p) => {
                self.page = p;
                self.list_open = false;
            }
            Message::Tick => {
                self.now = Utc::now();
                // Catches up after a suspend, which stalls the timer.
                if let Some(id) = self.panel_id()
                    && self.scheduler.due.is_some_and(|d| d <= self.now)
                    && !self.fetching(&id)
                {
                    tracing::debug!("refresh overdue");
                    return self.refresh(&id, Why::Scheduled);
                }
            }
            Message::Wake(generation) => {
                if generation == self.timer
                    && let Some(id) = self.panel_id()
                {
                    return self.refresh(&id, Why::Scheduled);
                }
            }
            Message::CacheLoaded(l) => {
                self.cache_loaded = true;
                tracing::debug!(cached = l.snapshots.len(), "cache loaded");
                for (id, s) in l.snapshots {
                    self.places.entry(id).or_default().snapshot = Some(s);
                }
                if let Some(b) = l.budget {
                    self.budget = b;
                }
                // The cached plan stands in until the key is checked.
                if self.tier.is_none() {
                    self.tier = self.panel_id().and_then(|id| self.places.get(&id)?.snapshot.as_ref().map(|s| s.tier));
                }
                return self.start();
            }
            Message::KeyLoaded(found) => {
                self.key_loaded = true;
                match found {
                    Some((k, storage)) => {
                        tracing::debug!(?storage, "key found");
                        self.key = Some(k.clone());
                        self.storage = Some(storage);
                        // Tier detection at startup (SPEC §3); it starts polling.
                        return self.check_key(k);
                    }
                    None => tracing::debug!("no key stored"),
                }
            }
            Message::KeyInput(s) => self.key_input = s,
            Message::KeyVisibility => self.key_hidden = !self.key_hidden,
            Message::KeyTest => {
                if self.key_status == KeyStatus::Checking {
                    return Task::none();
                }
                // A typed key is saved, then checked; with nothing typed, Test rechecks the stored one.
                let Some(k) = Key::new(&self.key_input) else {
                    return self.key.clone().map_or_else(Task::none, |k| self.check_key(k));
                };
                self.key_status = KeyStatus::Checking;
                let client = self.client.clone();
                return cosmic::task::future(async move {
                    let storage = key::store(&k).await;
                    let (check, calls) = api::detect_tier(client, k.clone()).await;
                    Message::KeyTested(k, storage, check, calls)
                });
            }
            Message::KeyTested(k, storage, check, calls) => {
                let counted = self.count(calls);
                if let Some(s) = storage {
                    tracing::debug!(storage = ?s, "key saved");
                    self.key = Some(k);
                    self.storage = Some(s);
                    self.key_input.clear();
                    self.key_loaded = true;
                }
                return Task::batch([counted, self.checked(check)]);
            }
            Message::KeyChecked(check, calls) => {
                let counted = self.count(calls);
                return Task::batch([counted, self.checked(check)]);
            }
            Message::Fetched(id, f) => return self.fetched(id, f),
            Message::ManualRefresh => {
                if let Some(id) = self.view.clone().or_else(|| self.panel_id())
                    && self.can_refresh(&id)
                {
                    tracing::debug!("manual refresh");
                    return self.refresh(&id, Why::Manual);
                }
            }
            Message::ToggleList => self.list_open = !self.list_open,
            Message::View(id) => {
                // Changes only the popup's view, never the panel city.
                self.list_open = false;
                self.view = Some(id.clone());
                return self.refresh_if_older(&id, self.interval());
            }
            Message::ToggleAlert(id) => {
                if !self.open_alerts.remove(&id) {
                    self.open_alerts.insert(id);
                }
            }
            Message::SetPanel(id) => {
                let locations = self.config.locations.clone();
                return self.save_locations(locations, Some(id));
            }
            Message::Remove(id) => {
                if self.config.locations.len() > 1 {
                    let locations: Vec<Location> = self.config.locations.iter().filter(|l| l.id != id).cloned().collect();
                    let panel = self.config.panel_location.clone().filter(|p| *p != id);
                    return self.save_locations(locations, panel);
                }
            }
            Message::SearchInput(s) => return self.search_input(s),
            Message::SearchFire(g) => return self.search_fire(g),
            Message::Searched(g, r, calls) => {
                let counted = self.count(calls);
                if g == self.search_gen {
                    self.search_abort = None;
                    self.results = match r {
                        Ok(p) if p.is_empty() => Search::NoMatches,
                        Ok(p) => Search::Results(p),
                        Err(e) => {
                            tracing::debug!(?e, "search failed");
                            Search::Unavailable
                        }
                    };
                }
                return counted;
            }
            Message::Add(i) => return self.add(i),
            Message::Segment(seg, e) => return self.segment(seg, e),
            Message::Show(t, on) => {
                return match t {
                    Toggle::City => self.save(|c, h| c.set_show_city(h, on)),
                    Toggle::HiLo => self.save(|c, h| c.set_show_hilo(h, on)),
                };
            }
            Message::OpenAttribution => {
                std::thread::spawn(|| {
                    if let Err(e) = std::process::Command::new("xdg-open").arg(ATTRIBUTION_URL).status() {
                        tracing::warn!("opening {ATTRIBUTION_URL}: {e}");
                    }
                });
            }
            Message::Output(name, height) => {
                if name.as_deref() == Some(self.core.applet.output_name.as_str())
                    && let Some(h) = height
                {
                    self.output_height = Some(h as f32);
                }
            }
            Message::Escape => {
                // The location list first, then settings, then the popup.
                if self.list_open {
                    self.list_open = false;
                } else if self.page == Page::Settings {
                    self.page = Page::Main;
                } else if let Some(id) = self.popup.take() {
                    return cosmic::task::message(cosmic::Action::Surface(destroy_popup(id)));
                }
            }
            #[cfg(feature = "demo")]
            Message::DemoNext => {
                if let Some(i) = self.demo {
                    self.show_scene(i + 1);
                }
            }
            #[cfg(feature = "demo")]
            Message::TakeShot => {
                if let Some(id) = self.core.main_window_id() {
                    return cosmic::iced::window::screenshot(id).map(|s| cosmic::Action::App(Message::Shot(s)));
                }
            }
            #[cfg(feature = "demo")]
            Message::Shot(s) => {
                if let Some(path) = crate::demo::shot_path() {
                    match crate::demo::save_shot(&path, &s) {
                        Ok(()) => tracing::info!("saved {}", path.display()),
                        Err(e) => tracing::error!("saving {}: {e}", path.display()),
                    }
                }
                std::process::exit(0);
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        if crate::preview() {
            #[cfg(feature = "demo")]
            if self.demo.is_some() {
                return widget::container(Column::new().push(self.panel_preview()).push(self.popup_view())).width(Length::Fixed(360.0)).into();
            }
            return widget::container(self.popup_view()).width(Length::Fixed(360.0)).into();
        }
        self.panel_view()
    }

    fn view_window(&self, _id: Id) -> Element<'_, Message> {
        widget::text("").into()
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        (!crate::preview()).then(applet::style)
    }
}

/// Esc, and the outputs' sizes (for scrolling on short screens).
fn events() -> Subscription<Message> {
    use cosmic::iced::event::{PlatformSpecific, wayland};
    cosmic::iced::event::listen_with(|event, _, _| match event {
        cosmic::iced::Event::Keyboard(keyboard::Event::KeyPressed { key: keyboard::Key::Named(Named::Escape), .. }) => Some(Message::Escape),
        cosmic::iced::Event::PlatformSpecific(PlatformSpecific::Wayland(wayland::Event::Output(
            wayland::OutputEvent::Created(Some(info)) | wayland::OutputEvent::InfoUpdate(info),
            _,
        ))) => Some(Message::Output(info.name.clone(), info.logical_size.map(|s| s.1))),
        _ => None,
    })
}

fn open_popup(bounds: Rectangle, offset: cosmic::iced::Vector) -> cosmic::surface::Action<Message> {
    app_popup::<App>(
        |_| Default::default(),
        move |state: &mut App| {
            let new_id = Id::unique();
            state.popup = Some(new_id);
            let parent = state.core.main_window_id().unwrap_or(Id::RESERVED);
            let mut settings = state.core.applet.get_popup_settings(parent, new_id, None, None, None);
            settings.positioner.anchor_rect =
                Rectangle { x: (bounds.x - offset.x) as i32, y: (bounds.y - offset.y) as i32, width: bounds.width as i32, height: bounds.height as i32 };
            settings
        },
        Some(Box::new(|state: &App| Element::from(state.core.applet.popup_container(state.popup_view())).map(cosmic::Action::App))),
    )
}

#[cfg(test)]
mod tests {
    const SOURCES: [&str; 13] = [
        include_str!("app.rs"),
        include_str!("api/mod.rs"),
        include_str!("format.rs"),
        include_str!("icons.rs"),
        include_str!("widgets/mod.rs"),
        include_str!("widgets/alert.rs"),
        include_str!("widgets/banner.rs"),
        include_str!("widgets/current.rs"),
        include_str!("widgets/daily.rs"),
        include_str!("widgets/details.rs"),
        include_str!("widgets/header.rs"),
        include_str!("widgets/hourly.rs"),
        include_str!("widgets/panel.rs"),
    ];

    #[test]
    fn no_hex_colours() {
        // ACCEPTANCE C-02: colours come from `theme.cosmic()`.
        for src in SOURCES {
            assert!(!src.contains(concat!("Color::from_rgb", "(")), "hard-coded colour");
            assert!(!src.contains(concat!("color", "!(")), "hard-coded colour");
        }
    }

    #[test]
    fn every_string_key_exists() {
        // ACCEPTANCE C-03: every `fl!` key is in the .ftl file.
        let ftl = include_str!("../i18n/en/cosmic_applet_weather.ftl");
        let keys: Vec<&str> = ftl.lines().filter_map(|l| l.split_once(" = ").map(|(k, _)| k.trim())).collect();
        for src in SOURCES {
            for part in src.split(concat!("fl", "!(\"")).skip(1) {
                let key = part.split('"').next().unwrap();
                assert!(keys.contains(&key), "{key} missing from the .ftl file");
            }
        }
    }
}
