//! The applet: state, update, the panel button and the popup.

use cosmic::app::{Core, Task};
use cosmic::cosmic_config;
use cosmic::iced::futures::StreamExt;
use cosmic::iced::keyboard::{self, key::Named};
use cosmic::iced::window::Id;
use cosmic::iced::{Alignment, Length, Rectangle, Subscription};
use cosmic::surface::action::{app_popup, destroy_popup};
use cosmic::widget::segmented_button::{self, Entity, SingleSelectModel};
use cosmic::widget::{self, Column, Row};
use cosmic::{Element, applet, theme};

use common::ink::Ink;
use common::panel::Panel;
use common::ui::{self, space_m, space_xs, space_xxs, space_xxxs};

use chrono::{DateTime, TimeDelta, Utc};

use crate::api::{self, Outcome};
use crate::auth::NoLogin;
use crate::auth::{self, Account, Auth, Paths};
use crate::config::{APP_ID, Amount, Config, PanelStyle, REFRESH_CHOICES, ResetFormat, TIME_APPLET, TimeConfig};
use crate::fl;
use crate::format::{self, Live};
use crate::model::{Kind, Snapshot, State, Usage};
use crate::scheduler::{self, Scheduler};
use crate::widgets::banner::banner;
use crate::widgets::header::{Header, header};
use crate::widgets::panel::{self, Chunk, Look, Value};
use crate::widgets::row::{self, Ctx, lower_first, name, quota_row};

/// Data older than this isn't shown at all (SPEC §9).
const DISCARD_AFTER: TimeDelta = TimeDelta::hours(24);

/// The robot: the panel icon, the popup header icon and the desktop entry icon.
const ROBOT: &[u8] = include_bytes!("../resources/icons/hicolor/scalable/apps/io.github.dc.CosmicAppletAiUsage-symbolic.svg");

pub fn robot<'a, M: 'a>(px: u16) -> Element<'a, M> {
    widget::icon(widget::icon::from_svg_bytes(ROBOT).symbolic(true)).size(px).into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Main,
    Settings,
}

/// A segmented control on the settings page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seg {
    Style,
    Amount,
    Reset,
    Refresh,
}

/// A "Show in panel" toggle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toggle {
    Window(Kind),
    SessionReset,
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
    /// Read the login and fetch.
    Refresh,
    CredentialsChanged,
    Fetched(Outcome),
    /// The header's refresh button.
    ManualRefresh,
    /// The 30 s local clock: countdowns, pace and freshness.
    Tick,
    /// A planned fetch is due, if the generation still matches.
    Wake(u64),
    Show(Toggle, bool),
    /// Demo mode: the next scene.
    #[cfg(feature = "demo")]
    DemoNext,
    Segment(Seg, Entity),
    CopyDiagnostics,
}

pub struct App {
    core: Core,
    popup: Option<Id>,
    page: Page,
    config: Config,
    handler: Option<cosmic_config::Config>,
    paths: Option<Paths>,
    client: reqwest::Client,
    account: Account,
    state: State,
    snapshot: Option<Snapshot>,
    /// A request is in flight.
    fetching: bool,
    /// The credentials changed during a request: read them again after it.
    refetch: bool,
    scheduler: Scheduler,
    /// Bumped whenever the planned fetch changes; stale wake-ups are ignored.
    timer: u64,
    /// When the current login expires.
    expires_at: Option<DateTime<Utc>>,
    /// The clock the view uses, updated every 30 s.
    now: DateTime<Utc>,
    /// 24-hour clock, from the time applet.
    military: bool,
    style_model: SingleSelectModel,
    amount_model: SingleSelectModel,
    reset_model: SingleSelectModel,
    refresh_model: SingleSelectModel,
    /// Demo mode: the scene on show.
    #[cfg(feature = "demo")]
    demo: Option<usize>,
}

impl App {
    fn set_state(&mut self, state: State) {
        if state != self.state {
            tracing::debug!(from = self.state.name(), to = state.name(), "state");
            self.state = state;
        }
    }

    fn interval(&self) -> TimeDelta {
        TimeDelta::minutes(i64::from(self.config.refresh_minutes))
    }

    /// Plans the next fetch. Replaces any earlier plan.
    fn arm(&mut self, due: Option<DateTime<Utc>>) -> Task<Message> {
        self.timer += 1;
        let Some(due) = due else {
            tracing::debug!("polling paused until the credentials change");
            return Task::none();
        };
        let wait = (due - Utc::now()).to_std().unwrap_or_default();
        tracing::debug!(at = %due, in_s = wait.as_secs(), "next fetch");
        let generation = self.timer;
        cosmic::task::future(async move {
            tokio::time::sleep(wait).await;
            Message::Wake(generation)
        })
    }

    /// A redraw when the refresh button's debounce or pause ends.
    fn redraw_at(&self, at: Option<DateTime<Utc>>) -> Task<Message> {
        let Some(wait) = at.and_then(|t| (t - Utc::now()).to_std().ok()) else { return Task::none() };
        cosmic::task::future(async move {
            tokio::time::sleep(wait).await;
            Message::Tick
        })
    }

    #[cfg(feature = "demo")]
    fn show_scene(&mut self, i: usize) {
        let s = crate::demo::scene(i, Utc::now());
        tracing::debug!(scene = i, state = s.state.name(), "demo");
        self.demo = Some(i);
        self.now = Utc::now();
        self.account = s.account;
        self.snapshot = s.snapshot;
        self.state = s.state;
    }

    fn demo(&self) -> bool {
        #[cfg(feature = "demo")]
        return self.demo.is_some();
        #[cfg(not(feature = "demo"))]
        false
    }

    /// Reads the login (fresh each time) and starts a request if it's usable.
    fn refresh(&mut self) -> Task<Message> {
        if self.demo() {
            return Task::none();
        }
        if self.fetching {
            return Task::none();
        }
        let Some(paths) = &self.paths else {
            self.set_state(State::NotSignedIn(auth::NoLogin::Missing));
            return self.halt();
        };
        match auth::read(paths, Utc::now()) {
            Auth::NotSignedIn(why) => {
                self.account = Account::default();
                self.set_state(State::NotSignedIn(why));
                self.halt()
            }
            Auth::Expired(account) => {
                self.account = account;
                self.set_state(State::Expired);
                self.halt()
            }
            Auth::Ready(account, login) => {
                self.account = account;
                self.expires_at = login.expires_at;
                self.fetching = true;
                tracing::debug!("fetching usage");
                cosmic::task::future(api::fetch(self.client.clone(), login.token)).map(|o| cosmic::Action::App(Message::Fetched(o)))
            }
        }
    }

    /// No usable login: stop polling until the credentials file changes.
    fn halt(&mut self) -> Task<Message> {
        let due = self.scheduler.completed(scheduler::Result::Halt, Utc::now(), self.interval(), 0.0, []);
        self.arm(due)
    }

    fn fetched(&mut self, outcome: Outcome) -> Task<Message> {
        self.fetching = false;
        let now = Utc::now();
        self.now = now;
        let result = match &outcome {
            Outcome::Usage(_) | Outcome::Unrecognised(_) => scheduler::Result::Ok,
            Outcome::Unauthorized => scheduler::Result::Halt,
            Outcome::RateLimited(wait) => scheduler::Result::RateLimited(*wait),
            Outcome::Offline => scheduler::Result::Offline,
        };
        match outcome {
            Outcome::Usage(usage) => {
                tracing::debug!(windows = ?usage.windows.iter().map(|w| (w.kind, w.used)).collect::<Vec<_>>(), "usage");
                self.snapshot = Some(Snapshot { usage, fetched_at: now });
                self.set_state(State::Normal);
            }
            Outcome::Unauthorized => self.set_state(State::Expired),
            Outcome::RateLimited(wait) => self.set_state(State::RateLimited(now + wait)),
            Outcome::Offline => self.set_state(State::Offline),
            Outcome::Unrecognised(d) => {
                tracing::warn!(status = d.status, keys = ?d.keys, "usage format not recognised");
                self.set_state(State::Unrecognised(d));
            }
        }
        // Extra fetches just after each window resets, and when the login expires.
        let resets = self.snapshot.iter().flat_map(|s| s.usage.windows.iter().filter_map(|w| w.resets_at)).map(|t| t + scheduler::AFTER_RESET);
        let wakeups: Vec<_> = resets.chain(self.expires_at).collect();
        let due = self.scheduler.completed(result, now, self.interval(), scheduler::jitter(), wakeups);
        let mut tasks = vec![self.arm(due), self.redraw_at(self.scheduler.unblocks_at(now))];
        if std::mem::take(&mut self.refetch) {
            tasks.push(self.refresh());
        }
        Task::batch(tasks)
    }

    /// Whether the refresh button is enabled.
    fn can_refresh(&self) -> bool {
        self.scheduler.can_refresh(Utc::now(), self.fetching)
    }

    fn save<T>(&mut self, set: impl FnOnce(&mut Config, &cosmic_config::Config) -> Result<T, cosmic_config::Error>) {
        if let Some(h) = &self.handler
            && let Err(e) = set(&mut self.config, h)
        {
            tracing::error!("saving config: {e}");
        }
    }

    /// Selects the segments that match the settings.
    fn sync_models(&mut self) {
        fn select<T: PartialEq + 'static>(m: &mut SingleSelectModel, want: &T) {
            let found = m.iter().find(|e| m.data::<T>(*e) == Some(want));
            if let Some(e) = found {
                m.activate(e);
            }
        }
        select(&mut self.style_model, &self.config.style);
        select(&mut self.amount_model, &self.config.amount);
        select(&mut self.reset_model, &self.config.reset_format);
        select(&mut self.refresh_model, &self.config.refresh_minutes);
    }

    fn segment(&mut self, seg: Seg, e: Entity) -> Task<Message> {
        match seg {
            Seg::Style => {
                if let Some(v) = self.style_model.data::<PanelStyle>(e).copied() {
                    self.save(|c, h| c.set_style(h, v));
                }
            }
            Seg::Amount => {
                if let Some(v) = self.amount_model.data::<Amount>(e).copied() {
                    self.save(|c, h| c.set_amount(h, v));
                }
            }
            Seg::Reset => {
                if let Some(v) = self.reset_model.data::<ResetFormat>(e).copied() {
                    self.save(|c, h| c.set_reset_format(h, v));
                }
            }
            Seg::Refresh => {
                if let Some(v) = self.refresh_model.data::<u8>(e).copied()
                    && v != self.config.refresh_minutes
                {
                    self.save(|c, h| c.set_refresh_minutes(h, v));
                    self.sync_models();
                    let due = self.scheduler.retime(Utc::now(), self.interval(), scheduler::jitter());
                    return self.arm(due);
                }
            }
        }
        self.sync_models();
        Task::none()
    }

    /// The account has no Fable limit (known only once data has arrived).
    fn no_fable(&self) -> bool {
        self.usage().is_some_and(|u| u.get(Kind::Fable).is_none())
    }

    /// The windows to show, as of `now`: none when signed out, when the
    /// format wasn't recognised, or when the data is over a day old.
    fn live(&self) -> Vec<Live> {
        match (&self.state, self.usage()) {
            (State::NotSignedIn(_) | State::Unrecognised(_), _) | (_, None) => Vec::new(),
            (_, Some(u)) => u.windows.iter().map(|w| Live::of(w, self.now)).collect(),
        }
    }

    fn usage(&self) -> Option<&Usage> {
        self.snapshot.as_ref().filter(|s| self.now - s.fetched_at <= DISCARD_AFTER).map(|s| &s.usage)
    }

    /// Showing the last values while they can't be updated.
    fn stale(&self) -> bool {
        matches!(self.state, State::Offline | State::RateLimited(_) | State::Expired)
    }

    fn left(&self) -> bool {
        self.config.amount == Amount::Left
    }

    fn enabled(&self, kind: Kind) -> bool {
        match kind {
            Kind::Session => self.config.show_session,
            Kind::Weekly => self.config.show_weekly,
            Kind::Fable => self.config.show_fable,
        }
    }

    /// The panel's chunks, in the fixed order, then the session countdown.
    fn chunks(&self, vertical_xs: bool) -> Vec<Chunk> {
        let live = self.live();
        let left = self.left();
        let mut chunks: Vec<Chunk> = live
            .iter()
            .filter(|l| self.enabled(l.kind))
            .map(|l| Chunk {
                label: label(l.kind),
                value: Value::Window {
                    shown: l.shown(left),
                    tick: l.pace(self.now).map(|p| if left { 100.0 - p.expected } else { p.expected }),
                    level: l.level(),
                },
            })
            .collect();
        if self.config.show_session_reset
            && !vertical_xs
            && !chunks.is_empty()
            && let Some(s) = live.iter().find(|l| l.kind == Kind::Session)
        {
            chunks.push(Chunk { label: fl!("label-reset"), value: Value::Reset(format::compact_reset(s.resets_at, self.now)) });
        }
        chunks
    }

    /// The tooltip and accessible name.
    fn tooltip(&self) -> String {
        match &self.state {
            State::NotSignedIn(_) => return fl!("a11y-signed-out"),
            State::Unrecognised(_) => return fl!("a11y-format"),
            _ => {}
        }
        let left = self.left();
        let amount = if left { fl!("left") } else { fl!("used") };
        let windows: Vec<String> = self
            .live()
            .iter()
            .filter(|l| self.enabled(l.kind))
            .map(|l| {
                let pct = l.shown(left).round() as i64;
                match l.resets_at {
                    Some(t) => {
                        let reset = format::reset_text(t, self.now, false, self.military, &chrono::Local);
                        fl!("a11y-window", name = row::name(l.kind), pct = pct, amount = amount.clone(), reset = lower_first(&reset))
                    }
                    None => fl!("a11y-window-no-reset", name = row::name(l.kind), pct = pct, amount = amount.clone()),
                }
            })
            .collect();
        if windows.is_empty() {
            return if self.state == State::Loading { fl!("a11y-loading") } else { fl!("a11y-signed-out") };
        }
        let suffix = match &self.state {
            State::Offline => Some(match self.snapshot.as_ref().and_then(|s| format::ago(self.now - s.fetched_at)) {
                Some(t) => fl!("a11y-offline", time = t),
                None => fl!("a11y-offline-never"),
            }),
            State::RateLimited(until) => Some(fl!("a11y-rate-limited", time = format::countdown(*until, self.now))),
            State::Expired => Some(fl!("a11y-expired")),
            _ => None,
        };
        let mut text = windows.join("; ");
        if let Some(s) = suffix {
            text.push_str(" · ");
            text.push_str(&s);
        }
        text
    }

    // ---- panel ------------------------------------------------------------------------

    fn panel_view(&self) -> Element<'_, Message> {
        let p = Panel::of(&self.core.applet);
        let (major, minor) = self.core.applet.suggested_padding(true);
        let (iw, ih) = p.icon;

        let thickness = if p.horizontal { ih } else { iw } + 2.0 * f32::from(minor);
        let look = Look::of(&p, self.config.style, self.stale(), thickness);
        let chunks = self.chunks(p.vertical_xs());
        let content = panel::content(robot(if p.horizontal { ih } else { iw } as u16), &chunks, &look);

        let (pad, w, h) = if p.horizontal {
            ([0, major], Length::Shrink, Length::Fixed(ih + 2.0 * f32::from(minor)))
        } else {
            ([major, 0], Length::Fixed(iw + 2.0 * f32::from(minor)), Length::Shrink)
        };
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

    fn header(&self) -> Element<'_, Message> {
        let signed_out = matches!(self.state, State::NotSignedIn(_));
        let fresh = |warn: bool| match &self.snapshot {
            Some(s) => (format::freshness(s.fetched_at, self.now), warn),
            None => (fl!("updated-never"), warn),
        };
        let ago = self.snapshot.as_ref().and_then(|s| format::ago(self.now - s.fetched_at));
        let status = match &self.state {
            State::NotSignedIn(_) => None,
            _ if self.fetching && self.snapshot.is_none() => Some((fl!("refreshing"), false)),
            State::Loading | State::Normal | State::Unrecognised(_) => Some(fresh(false)),
            State::Expired => Some(fresh(true)),
            State::Offline => Some((
                match ago {
                    Some(t) => fl!("offline-ago", time = t),
                    None if self.snapshot.is_some() => fl!("offline-just-now"),
                    None => fl!("offline"),
                },
                true,
            )),
            State::RateLimited(until) => Some((fl!("rate-limited-retry", time = format::countdown(*until, self.now)), true)),
        };
        header(
            Header {
                plan: if signed_out { None } else { self.account.plan.clone() },
                subtitle: if signed_out { Some(fl!("not-signed-in")) } else { self.account.email.clone() },
                status,
                can_refresh: self.can_refresh(),
                fetching: self.fetching,
            },
            robot(20),
            Message::ManualRefresh,
        )
    }

    fn banner(&self) -> Option<Element<'_, Message>> {
        Some(match &self.state {
            State::NotSignedIn(NoLogin::NoProfileScope) => banner(fl!("signin-title"), fl!("login-no-profile"), None),
            State::NotSignedIn(_) => banner(fl!("signin-title"), fl!("signin-body"), None),
            State::Expired => banner(fl!("expired-title"), fl!("expired-body"), None),
            State::Unrecognised(_) => banner(fl!("format-title"), fl!("format-body"), Some((fl!("copy-diagnostics"), Message::CopyDiagnostics))),
            _ => return None,
        })
    }

    fn main_page(&self) -> Element<'_, Message> {
        let ctx =
            Ctx { now: self.now, left: self.left(), absolute: self.config.reset_format == ResetFormat::Absolute, military: self.military, stale: self.stale() };
        let live = self.live();
        let mut col = Column::new().push(self.header()).push(ui::inset_divider()).push_maybe(self.banner());
        for l in &live {
            col = col.push(quota_row(l, &ctx));
        }
        if !live.is_empty() && self.no_fable() {
            col = col.push(widget::container(widget::text::caption(fl!("no-fable-limit")).class(Ink::Muted.text())).padding([space_xxs(), space_m()]));
        }
        col.push(ui::inset_divider()).push(ui::settings_row(fl!("applet-settings"), Message::Page(Page::Settings))).into()
    }

    fn settings_page(&self) -> Element<'_, Message> {
        let windows_on = Kind::ALL.iter().filter(|k| self.enabled(**k)).count();
        let mut toggles = Column::new();
        let rows = [
            (Toggle::Window(Kind::Session), name(Kind::Session), fl!("session-desc")),
            (Toggle::Window(Kind::Weekly), name(Kind::Weekly), fl!("toggle-weekly-desc")),
            (Toggle::Window(Kind::Fable), name(Kind::Fable), fl!("toggle-fable-desc")),
            (Toggle::SessionReset, fl!("session-reset"), fl!("session-reset-desc")),
        ];
        for (t, title, caption) in rows {
            let checked = match t {
                Toggle::Window(k) => self.enabled(k),
                Toggle::SessionReset => self.config.show_session_reset,
            };
            let last = matches!(t, Toggle::Window(_)) && checked && windows_on == 1;
            let unavailable = t == Toggle::Window(Kind::Fable) && self.no_fable();
            let caption = if unavailable {
                fl!("no-fable-limit")
            } else if last {
                fl!("at-least-one")
            } else {
                caption
            };
            let toggler = widget::toggler(checked).on_toggle_maybe((!last && !unavailable).then_some(move |v| Message::Show(t, v)));
            toggles = toggles.push(applet::padded_control(
                Row::new()
                    .spacing(space_xs())
                    .align_y(Alignment::Center)
                    .push(Column::new().width(Length::Fill).push(widget::text::body(title)).push(widget::text::caption(caption).class(Ink::Muted.text())))
                    .push(toggler),
            ));
        }
        fn seg(model: &SingleSelectModel, s: Seg) -> Element<'_, Message> {
            applet::padded_control(
                widget::segmented_control::horizontal(model).button_padding([space_xxxs(), 0, space_xxxs(), 0]).on_activate(move |e| Message::Segment(s, e)),
            )
            .into()
        }
        Column::new()
            .push(ui::back_header(fl!("applet-settings"), fl!("back"), Message::Page(Page::Main)))
            .push(ui::group_label(fl!("show-in-panel")))
            .push(toggles)
            .push(ui::inset_divider())
            .push(ui::group_label(fl!("panel-style")))
            .push(seg(&self.style_model, Seg::Style))
            .push(ui::inset_divider())
            .push(ui::group_label(fl!("show")))
            .push(seg(&self.amount_model, Seg::Amount))
            .push(ui::inset_divider())
            .push(ui::group_label(fl!("reset-times")))
            .push(seg(&self.reset_model, Seg::Reset))
            .push(ui::inset_divider())
            .push(ui::group_label(fl!("refresh-every")))
            .push(seg(&self.refresh_model, Seg::Refresh))
            .into()
    }
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
        tracing::debug!(?config, "config loaded");
        let app = Self {
            core,
            popup: None,
            page: if crate::preview_settings() { Page::Settings } else { Page::Main },
            config,
            handler,
            paths: Paths::discover(),
            client: api::client(),
            account: Account::default(),
            state: State::Loading,
            snapshot: None,
            fetching: false,
            refetch: false,
            scheduler: Scheduler::default(),
            timer: 0,
            expires_at: None,
            now: Utc::now(),
            military: TimeConfig::load().military_time,
            style_model: segmented_button::Model::builder()
                .insert(|b| b.text(fl!("style-percent")).data(PanelStyle::Percent))
                .insert(|b| b.text(fl!("style-bars")).data(PanelStyle::Bars))
                .insert(|b| b.text(fl!("style-both")).data(PanelStyle::Both))
                .build(),
            amount_model: segmented_button::Model::builder()
                .insert(|b| b.text(fl!("amount-used")).data(Amount::Used))
                .insert(|b| b.text(fl!("amount-left")).data(Amount::Left))
                .build(),
            reset_model: segmented_button::Model::builder()
                .insert(|b| b.text(fl!("reset-relative")).data(ResetFormat::Relative))
                .insert(|b| b.text(fl!("reset-absolute")).data(ResetFormat::Absolute))
                .build(),
            #[cfg(feature = "demo")]
            demo: None,
            refresh_model: {
                let mut m = segmented_button::Model::builder().build();
                for n in REFRESH_CHOICES {
                    m.insert().text(fl!("minutes", n = n)).data(n);
                }
                m
            },
        };
        let mut app = app;
        app.sync_models();
        #[cfg(feature = "demo")]
        if crate::demo::enabled() {
            app.show_scene(crate::demo::first());
            return (app, Task::none());
        }
        (app, cosmic::task::message(Message::Refresh))
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn subscription(&self) -> Subscription<Message> {
        #[cfg(feature = "demo")]
        if self.demo.is_some() {
            return Subscription::batch([
                cosmic::iced::time::every(crate::demo::STEP).map(|_| Message::DemoNext),
                cosmic::iced::time::every(std::time::Duration::from_secs(30)).map(|_| Message::Tick),
                self.core.watch_config::<Config>(APP_ID).map(|u| Message::Config(u.config)),
                self.core.watch_config::<TimeConfig>(TIME_APPLET).map(|u| Message::TimeConfig(u.config)),
                escape(),
            ]);
        }
        let watch = self.paths.as_ref().map(|p| Subscription::run_with(p.credentials.clone(), |c| auth::watch(c).map(|()| Message::CredentialsChanged)));
        Subscription::batch([
            watch.unwrap_or_else(Subscription::none),
            cosmic::iced::time::every(std::time::Duration::from_secs(30)).map(|_| Message::Tick),
            self.core.watch_config::<Config>(APP_ID).map(|u| Message::Config(u.config)),
            self.core.watch_config::<TimeConfig>(TIME_APPLET).map(|u| Message::TimeConfig(u.config)),
            escape(),
        ])
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Surface(a) => return cosmic::task::message(cosmic::Action::Surface(a)),
            Message::TogglePopup(bounds, offset) => {
                if let Some(id) = self.popup.take() {
                    return cosmic::task::message(cosmic::Action::Surface(destroy_popup(id)));
                }
                self.page = Page::Main;
                let open = cosmic::task::message(cosmic::Action::Surface(open_popup(bounds, offset)));
                let halted = matches!(self.state, State::Expired | State::NotSignedIn(_));
                if !halted && self.scheduler.stale_on_open(Utc::now(), self.fetching) {
                    tracing::debug!("popup opened on old data");
                    return Task::batch([open, self.refresh()]);
                }
                return open;
            }
            Message::PopupClosed(id) => {
                if self.popup == Some(id) {
                    self.popup = None;
                }
            }
            Message::Config(c) => {
                let c = c.enforce();
                if c != self.config {
                    tracing::debug!(config = ?c, "config changed");
                    let retime = c.refresh_minutes != self.config.refresh_minutes;
                    self.config = c;
                    self.sync_models();
                    if retime {
                        let due = self.scheduler.retime(Utc::now(), self.interval(), scheduler::jitter());
                        return self.arm(due);
                    }
                }
            }
            Message::Page(p) => self.page = p,
            Message::TimeConfig(t) => self.military = t.military_time,
            Message::Refresh => return self.refresh(),
            Message::CredentialsChanged => {
                tracing::debug!("re-reading credentials");
                if self.fetching {
                    self.refetch = true;
                } else {
                    return self.refresh();
                }
            }
            Message::Fetched(o) => return self.fetched(o),
            Message::ManualRefresh => {
                if self.scheduler.can_refresh(Utc::now(), self.fetching) {
                    tracing::debug!("manual refresh");
                    return self.refresh();
                }
            }
            Message::Tick => {
                self.now = Utc::now();
                // Catches up after a suspend, which stalls the timer.
                if !self.fetching && self.scheduler.due.is_some_and(|d| d <= self.now) {
                    tracing::debug!("fetch overdue");
                    return self.refresh();
                }
            }
            Message::Show(t, on) => match t {
                Toggle::Window(Kind::Session) => self.save(|c, h| c.set_show_session(h, on)),
                Toggle::Window(Kind::Weekly) => self.save(|c, h| c.set_show_weekly(h, on)),
                Toggle::Window(Kind::Fable) => self.save(|c, h| c.set_show_fable(h, on)),
                Toggle::SessionReset => self.save(|c, h| c.set_show_session_reset(h, on)),
            },
            Message::Segment(seg, e) => return self.segment(seg, e),
            #[cfg(feature = "demo")]
            Message::DemoNext => {
                if let Some(i) = self.demo {
                    self.show_scene(i + 1);
                }
            }
            Message::CopyDiagnostics => {
                if let State::Unrecognised(d) = &self.state {
                    return cosmic::iced::clipboard::write(d.text());
                }
            }
            Message::Wake(generation) => {
                if generation == self.timer {
                    return self.refresh();
                }
            }
            Message::Escape => {
                if self.page == Page::Settings {
                    self.page = Page::Main;
                } else if let Some(id) = self.popup.take() {
                    return cosmic::task::message(cosmic::Action::Surface(destroy_popup(id)));
                }
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        if crate::preview() {
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

fn escape() -> Subscription<Message> {
    cosmic::iced::event::listen_with(|event, _, _| match event {
        cosmic::iced::Event::Keyboard(keyboard::Event::KeyPressed { key: keyboard::Key::Named(Named::Escape), .. }) => Some(Message::Escape),
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

/// Panel label: `5h`, `Week`, `Fable`.
fn label(kind: Kind) -> String {
    match kind {
        Kind::Session => fl!("label-session"),
        Kind::Weekly => fl!("label-weekly"),
        Kind::Fable => fl!("label-fable"),
    }
}

#[cfg(test)]
mod tests {
    const SOURCES: [&str; 8] = [
        include_str!("app.rs"),
        include_str!("format.rs"),
        include_str!("widgets/banner.rs"),
        include_str!("widgets/header.rs"),
        include_str!("widgets/meter.rs"),
        include_str!("widgets/panel.rs"),
        include_str!("widgets/row.rs"),
        include_str!("demo.rs"),
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
        let ftl = include_str!("../i18n/en/cosmic_applet_ai_usage.ftl");
        let keys: Vec<&str> = ftl.lines().filter_map(|l| l.split_once(" = ").map(|(k, _)| k.trim())).collect();
        for src in SOURCES {
            for part in src.split(concat!("fl", "!(\"")).skip(1) {
                let key = part.split('"').next().unwrap();
                assert!(keys.contains(&key), "{key} missing from the .ftl file");
            }
        }
    }
}
