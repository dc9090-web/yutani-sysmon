//! The applet: update, the panel button and the popup.

use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use cosmic::app::{Core, Task};
use cosmic::cosmic_config;
use cosmic::iced::font::Weight;
use cosmic::iced::keyboard::{self, key::Named};
use cosmic::iced::window::Id;
use cosmic::iced::{Alignment, Length, Rectangle, Subscription};
use cosmic::surface::action::{app_popup, destroy_popup};
use cosmic::widget::segmented_button::{self, Entity, SingleSelectModel};
use cosmic::widget::{self, Column, Row};
use cosmic::{Element, applet, theme};

use common::ink::Ink;
use common::panel::{Panel, Type};
use common::ui::{self, mono, space_m, space_s, space_xs, space_xxs, space_xxxs};

use crate::api::{self, ApiError, BusEvent, MaskedPrefs, Prefs, Status, Suggestion};
use crate::config::{APP_ID, Config, CopyWhat};
use crate::fl;
use crate::model::{self, Conn, Device, ExitOption, Link};
use crate::preview;
use crate::state::{BUSY_LIMIT, Banner, Busy, Page, State, copy_text};
use crate::widgets::glyph::glyph;
use crate::widgets::panel::{self, Chunk, tone_ink};

/// After a bus event, the status is fetched once things have settled.
const FETCH_DEBOUNCE: Duration = Duration::from_millis(500);
/// How often the device list is refreshed while the popup is open.
const REFRESH: Duration = Duration::from_secs(5);
const TOAST: Duration = Duration::from_millis(1400);
/// Width of the key column in "This device".
const KEY_W: f32 = 92.0;
/// The lists scroll past these heights, so the popup stays under libcosmic's cap.
const DEVICES_MAX: f32 = 336.0;
const PAGE_MAX: f32 = 640.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Toggle {
    ShowStatus,
    ShowOffline,
}

#[derive(Debug, Clone)]
pub enum Message {
    Surface(cosmic::surface::Action<Message>),
    TogglePopup(Rectangle, cosmic::iced::Vector),
    PopupClosed(Id),
    Config(Config),
    Page(Page),
    Escape,
    Bus(BusEvent),
    /// The debounce after a bus event has passed.
    FetchStatus,
    /// The popup's refresh timer.
    Refresh,
    Status(Box<Result<Status, ApiError>>),
    Switch(bool),
    SelectExit(String),
    AllowLan(bool),
    Patched(Result<Prefs, ApiError>),
    BusyCheck,
    Suggested(Option<Suggestion>),
    SignIn,
    SignInSent(Result<(), ApiError>),
    Copy(String),
    ToastEnd(u64),
    Query(String),
    ExitQuery(String),
    Set(Toggle, bool),
    CopyMode(Entity),
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
    config: Config,
    handler: Option<cosmic_config::Config>,
    st: State,
    /// A status fetch is already scheduled.
    fetch_pending: bool,
    toast: Option<(String, u64)>,
    toast_id: u64,
    copy_model: SingleSelectModel,
    #[cfg(feature = "demo")]
    demo: Option<usize>,
    /// The demo scene's stand-in for the operator check.
    #[cfg(feature = "demo")]
    demo_now: Option<DateTime<Utc>>,
}

fn current_user() -> (String, u32) {
    let uid = nix::unistd::getuid();
    let name = nix::unistd::User::from_uid(uid).ok().flatten().map(|u| u.name).or_else(|| std::env::var("USER").ok()).unwrap_or_default();
    (name, uid.as_raw())
}

impl App {
    fn demo(&self) -> bool {
        #[cfg(feature = "demo")]
        return self.demo.is_some();
        #[cfg(not(feature = "demo"))]
        false
    }

    fn now(&self) -> DateTime<Utc> {
        #[cfg(feature = "demo")]
        if let Some(t) = self.demo_now {
            return t;
        }
        Utc::now()
    }

    fn save(&mut self, set: impl FnOnce(&mut Config, &cosmic_config::Config) -> Result<bool, cosmic_config::Error>) {
        match &self.handler {
            Some(h) => {
                if let Err(e) = set(&mut self.config, h) {
                    tracing::error!("saving config: {e}");
                }
            }
            None => tracing::error!("no config handler; setting not saved"),
        }
    }

    fn toast(&mut self, text: String) -> Task<Message> {
        self.toast_id += 1;
        let id = self.toast_id;
        self.toast = Some((text, id));
        cosmic::task::future(async move {
            tokio::time::sleep(TOAST).await;
            Message::ToastEnd(id)
        })
    }

    fn sync_copy_model(&mut self) {
        let want = self.config.copy;
        let found = self.copy_model.iter().find(|e| self.copy_model.data::<CopyWhat>(*e) == Some(&want));
        if let Some(e) = found {
            self.copy_model.activate(e);
        }
    }

    fn fetch_now(&self) -> Task<Message> {
        if self.demo() {
            return Task::none();
        }
        cosmic::task::future(async { Message::Status(Box::new(api::status().await)) })
    }

    /// One fetch after the bus has gone quiet, however many events arrived.
    fn fetch_soon(&mut self) -> Task<Message> {
        if self.fetch_pending || self.demo() {
            return Task::none();
        }
        self.fetch_pending = true;
        cosmic::task::future(async {
            tokio::time::sleep(FETCH_DEBOUNCE).await;
            Message::FetchStatus
        })
    }

    /// Sends a prefs write, with the timer that gives up on it.
    fn patch(&self, prefs: MaskedPrefs) -> Task<Message> {
        if self.demo() {
            return Task::none();
        }
        let write = cosmic::task::future(async move { Message::Patched(api::patch_prefs(prefs).await) });
        let limit = cosmic::task::future(async {
            tokio::time::sleep(BUSY_LIMIT).await;
            Message::BusyCheck
        });
        Task::batch([write, limit])
    }

    // ---- panel ------------------------------------------------------------------------

    fn chunk(&self) -> Option<Chunk> {
        let now = self.now();
        if !model::show_chunk(&self.st.snap, now, self.config.show_status) {
            return None;
        }
        let (key, tone) = model::word(&self.st.snap, now);
        Some(Chunk { label: fl!("panel-label"), value: fl_key(key), tone, chars: 3.0 })
    }

    fn tooltip(&self) -> String {
        let link = model::link(&self.st.snap);
        let mut text = match link {
            Link::Connected => fl!("tooltip-connected"),
            Link::Connecting => fl!("tooltip-connecting"),
            Link::Off => fl!("tooltip-off"),
            Link::NeedsLogin => fl!("tooltip-login"),
            Link::NeedsApproval => fl!("tooltip-approval"),
            Link::OtherUser => fl!("tooltip-other-user"),
            Link::Unavailable => fl!("tooltip-unavailable"),
        };
        if link == Link::Connected {
            let s = self.st.summary();
            if let Some(name) = s.exit {
                text.push(' ');
                text.push_str(&fl!("tooltip-exit", name = name));
            }
            if s.total > 0 {
                text.push(' ');
                text.push_str(&fl!("tooltip-devices", online = s.online, total = s.total));
            }
            if model::issue(&self.st.snap, self.now()).is_some() {
                text.push(' ');
                text.push_str(&fl!("tooltip-attention"));
            }
        }
        text
    }

    fn panel_view(&self) -> Element<'_, Message> {
        let p = Panel::of(&self.core.applet);
        let (major, minor) = self.core.applet.suggested_padding(true);
        let (iw, ih) = p.icon;
        let size = if p.horizontal { ih } else { iw };
        let thickness = size + 2.0 * f32::from(minor);
        let icon = glyph(model::icon(&self.st.snap), model::issue(&self.st.snap, self.now()), size);
        let content = panel::content(icon, self.chunk(), &p);
        let (pad, w, h) =
            if p.horizontal { ([0, major], Length::Shrink, Length::Fixed(thickness)) } else { ([major, 0], Length::Fixed(thickness), Length::Shrink) };
        let button = widget::button::custom(widget::container(content).center_x(w).center_y(h))
            .padding(pad)
            .class(theme::Button::AppletIcon)
            .on_press_with_rectangle(|offset, bounds| Message::TogglePopup(bounds, offset));
        let tip = self.core.applet.applet_tooltip::<Message>(button, self.tooltip(), self.popup.is_some(), Message::Surface, None);
        self.core.applet.autosize_window(tip).into()
    }

    #[cfg(feature = "demo")]
    fn panel_preview(&self) -> Element<'_, Message> {
        let p = Panel::of(&self.core.applet);
        let icon = glyph(model::icon(&self.st.snap), model::issue(&self.st.snap, self.now()), 20.0);
        let content = panel::content(icon, self.chunk(), &p);
        widget::container(widget::container(content).padding([4, 12]).class(theme::Container::Card)).padding([8, 24]).into()
    }

    // ---- popup ------------------------------------------------------------------------

    fn popup_view(&self) -> Element<'_, Message> {
        let content = match self.st.page {
            Page::Main => self.main_page(),
            Page::ExitNode => self.exit_page(),
            Page::Settings => self.settings_page(),
        };
        let mut col = Column::new().push(content);
        if let Some((t, _)) = &self.toast {
            col = col.push(widget::container(toast(t.clone())).center_x(Length::Fill).padding([space_xxs(), 0]));
        }
        widget::container(col).padding([8, 0, 8, 0]).into()
    }

    fn header(&self) -> Element<'_, Message> {
        let link = model::link(&self.st.snap);
        let (key, tone) = model::header(&self.st.snap);
        let suffix = self.st.snap.status.tailnet.as_ref().map(|t| t.magic_dns_suffix.clone()).filter(|s| !s.is_empty());
        let caption = match (link, suffix) {
            (Link::Connected, Some(tailnet)) => fl!("head-connected-tailnet", tailnet = tailnet),
            _ => fl_key(key),
        };
        let ink = tone.map_or(Ink::Muted, tone_ink);
        let on = matches!(link, Link::Connected | Link::Connecting);
        // Usable while connecting too, so a start that hangs can be cancelled.
        let usable = matches!(link, Link::Connected | Link::Connecting | Link::Off) && self.st.can_write() && self.st.busy.is_none() && !self.demo();
        let toggler = widget::toggler(on).on_toggle_maybe(usable.then_some(Message::Switch as fn(bool) -> Message));
        widget::container(
            Row::new()
                .spacing(space_s())
                .align_y(Alignment::Center)
                .push(glyph(model::icon(&self.st.snap), None, 24.0))
                .push(Column::new().width(Length::Fill).push(widget::text::heading(fl!("title"))).push(widget::text::caption(caption).class(ink.text())))
                .push(toggler),
        )
        .padding([space_xs(), space_m()])
        .into()
    }

    fn banner(&self, b: Banner) -> Element<'_, Message> {
        match b {
            Banner::NotRunning { command } => command_banner(fl!("not-running-title"), fl!("not-running-body"), command.to_owned()),
            Banner::Operator { command } => command_banner(fl!("operator-title"), fl!("operator-body"), command),
            Banner::SignIn => {
                let button = widget::button::standard(fl!("signin-button")).on_press_maybe((self.st.can_write() && !self.demo()).then_some(Message::SignIn));
                let col = Column::new()
                    .spacing(space_xxs())
                    .push(widget::text::body(fl!("signin-title")).font(cosmic::font::semibold()))
                    .push(widget::text::caption(fl!("signin-body")).class(Ink::Muted.text()))
                    .push(button);
                widget::container(boxed(col.into())).padding([space_xxs(), space_m()]).into()
            }
            Banner::KeyExpired => note(fl!("key-expired"), Ink::Destructive),
            Banner::KeyExpiring(0) => note(fl!("key-expiring-today"), Ink::Warning),
            Banner::KeyExpiring(days) => note(fl!("key-expiring", days = days), Ink::Warning),
            Banner::Error(text) => note(if text.is_empty() { fl!("err-timeout") } else { fl!("err-request", text = text) }, Ink::Destructive),
            Banner::Health(text) => note(text, Ink::Muted),
            Banner::HealthMore(count) => widget::container(widget::text::caption(fl!("health-more", count = count)).class(Ink::Muted.text()))
                .padding([0, space_m(), space_xxs(), space_m() + 16 + space_xs()])
                .into(),
        }
    }

    fn main_page(&self) -> Element<'_, Message> {
        let now = self.now();
        let mut col = Column::new().push(self.header());
        for b in self.st.banners(now) {
            col = col.push(self.banner(b));
        }
        let st = &self.st.snap.status;
        if st.have_node_key && self.st.snap.reachable {
            col = col
                .push(widget::divider::horizontal::default())
                .push(self.this_device())
                .push(ui::inset_divider())
                .push(self.exit_row())
                .push(ui::inset_divider());
            col = col.push(self.devices(now));
        }
        col.push(ui::inset_divider()).push(ui::settings_row(fl!("settings"), Message::Page(Page::Settings))).into()
    }

    fn this_device(&self) -> Element<'_, Message> {
        let Some(me) = self.st.snap.status.self_node.as_ref() else { return widget::space().into() };
        let ip = me.ips.iter().find(|ip| !ip.contains(':')).cloned();
        let dns = me.dns_name.trim_end_matches('.').to_owned();
        let host = if me.host_name.is_empty() { dns.split('.').next().unwrap_or_default().to_owned() } else { me.host_name.clone() };
        let mut col = Column::new().spacing(space_xxxs()).push(kv(fl!("this-device"), host, false));
        if let Some(ip) = ip {
            col = col.push(kv(fl!("address"), ip, true));
        }
        if !dns.is_empty() {
            col = col.push(kv(fl!("name"), dns, true));
        }
        widget::container(col).padding([space_xxs(), space_m()]).into()
    }

    fn exit_row(&self) -> Element<'_, Message> {
        let (value, ink) = if self.st.busy.is_some_and(|b| b.0 == Busy::Exit) {
            (fl!("exit-changing"), Ink::Muted)
        } else {
            match model::exit_name(&self.st.snap) {
                None => (fl!("exit-none"), Ink::Muted),
                Some((name, true)) => (name, Ink::On),
                Some((name, false)) if self.st.snap.status.peers.iter().any(|p| p.id == self.st.snap.prefs.exit_node_id) => {
                    (fl!("exit-offline", name = name), Ink::Warning)
                }
                Some((name, false)) => (name, Ink::On),
            }
        };
        let row = Row::new()
            .spacing(space_xs())
            .align_y(Alignment::Center)
            .push(widget::text::body(fl!("exit-node")).width(Length::Fixed(KEY_W)))
            .push(widget::text::body(value).class(ink.text()).width(Length::Fill))
            .push(widget::icon::from_name("go-next-symbolic").size(16).symbolic(true));
        applet::menu_button(row).on_press(Message::Page(Page::ExitNode)).into()
    }

    fn devices(&self, now: DateTime<Utc>) -> Element<'_, Message> {
        let st = &self.st.snap.status;
        let (online, total) = model::online_count(st);
        let mut col = Column::new().push(ui::group_label(fl!("devices-heading", online = online, total = total)));
        if self.st.show_search() {
            col = col.push(
                widget::container(
                    widget::search_input(fl!("search-devices"), self.st.query.as_str()).on_input(Message::Query).on_clear(Message::Query(String::new())),
                )
                .padding([space_xxs(), space_m()]),
            );
        }
        let list = model::devices(st, &self.st.query, self.config.show_offline);
        if list.is_empty() {
            let text = if total == 0 { fl!("devices-none") } else { fl!("devices-no-match") };
            return col.push(widget::container(widget::text::caption(text).class(Ink::Muted.text())).padding([space_xxs(), space_m()])).into();
        }
        let mut rows = Column::new();
        for d in &list {
            rows = rows.push(self.device_row(d, now));
        }
        col.push(widget::container(widget::scrollable(rows)).max_height(DEVICES_MAX)).into()
    }

    fn device_row(&self, d: &Device, now: DateTime<Utc>) -> Element<'_, Message> {
        let conn = match &d.conn {
            Conn::Direct => fl!("conn-direct"),
            Conn::PeerRelay => fl!("conn-peer-relay"),
            Conn::Relay(code) => fl!("conn-relay", code = code.clone()),
            Conn::Idle => fl!("conn-idle"),
            Conn::LastSeen(Some(t)) => fl!("conn-last-seen", ago = model::last_seen(*t, now)),
            Conn::LastSeen(None) => fl!("conn-offline"),
        };
        let name_ink = if d.online { Ink::On } else { Ink::Muted };
        let row = Row::new()
            .spacing(space_xs())
            .align_y(Alignment::Center)
            .push(ui::dot(if d.online { Ink::Success } else { Ink::ComponentDivider }))
            .push(widget::text::body(d.name.clone()).class(name_ink.text()).wrapping(cosmic::iced::widget::text::Wrapping::None))
            .push(widget::text::caption(d.os.clone()).class(Ink::Muted.text()).width(Length::Fill))
            .push(widget::text::caption(conn).class(Ink::Muted.text()));
        applet::menu_button(row).on_press_maybe(copy_text(d, self.config.copy).map(Message::Copy)).into()
    }

    // ---- exit node page -----------------------------------------------------------------

    fn exit_option(&self, o: &ExitOption, current: &str, now: DateTime<Utc>) -> Element<'_, Message> {
        let caption = match (o.online, &o.caption, o.last_seen) {
            (true, Some(city), _) => Some((city.clone(), Ink::Muted)),
            (true, None, _) => None,
            (false, _, Some(t)) => Some((fl!("exit-offline-seen", ago = model::last_seen(t, now)), Ink::Muted)),
            (false, _, None) => Some((fl!("exit-offline-plain"), Ink::Muted)),
        };
        let selected = o.id == current;
        // An offline node can't be picked, but the one in use can still be seen selected.
        let msg = (o.online && self.st.can_write() && !self.demo()).then(|| Message::SelectExit(o.id.clone()));
        radio(selected, o.name.clone(), caption, o.online, msg)
    }

    fn exit_page(&self) -> Element<'_, Message> {
        let now = self.now();
        let st = &self.st.snap.status;
        let current = self.st.snap.prefs.exit_node_id.as_str();
        let writable = self.st.can_write() && !self.demo();
        let mut body = Column::new();
        if self.st.show_exit_search() {
            body = body.push(
                widget::container(
                    widget::search_input(fl!("search-exit"), self.st.exit_query.as_str())
                        .on_input(Message::ExitQuery)
                        .on_clear(Message::ExitQuery(String::new())),
                )
                .padding([space_xxs(), space_m()]),
            );
        }
        let searching = !self.st.exit_query.trim().is_empty();
        if !searching {
            body = body.push(radio(current.is_empty(), fl!("exit-none"), None, true, writable.then(|| Message::SelectExit(String::new()))));
            if let Some((id, name)) = self.st.recommended() {
                let id = id.to_owned();
                body = body.push(radio(false, fl!("exit-recommended", name = name), None, true, writable.then(|| Message::SelectExit(id))));
            }
        }
        let groups = model::exit_groups(st, &self.st.exit_query);
        if groups.is_empty() {
            let text = if searching { fl!("exit-no-match") } else { fl!("exit-no-options") };
            body = body.push(widget::container(widget::text::caption(text).class(Ink::Muted.text())).padding([space_xxs(), space_m()]));
        }
        let located = groups.iter().any(|g| g.heading.is_some());
        for g in &groups {
            match &g.heading {
                Some(country) => body = body.push(ui::group_label(country.clone())),
                None if located => body = body.push(ui::group_label(fl!("exit-own"))),
                None => {}
            }
            for o in &g.options {
                body = body.push(self.exit_option(o, current, now));
            }
        }
        let lan = applet::padded_control(
            Row::new()
                .spacing(space_xs())
                .align_y(Alignment::Center)
                .push(
                    Column::new()
                        .width(Length::Fill)
                        .push(widget::text::body(fl!("exit-lan-title")))
                        .push(widget::text::caption(fl!("exit-lan-caption")).class(Ink::Muted.text())),
                )
                .push(
                    widget::toggler(self.st.snap.prefs.exit_node_allow_lan)
                        .on_toggle_maybe((self.st.lan_enabled() && self.st.busy.is_none() && !self.demo()).then_some(Message::AllowLan as fn(bool) -> Message)),
                ),
        );
        let mut col = Column::new().push(ui::back_header(fl!("exit-node"), fl!("back"), Message::Page(Page::Main)));
        // Why the rows don't respond, when they don't.
        for b in self.st.banners(now).into_iter().filter(|b| matches!(b, Banner::Operator { .. } | Banner::Error(_))) {
            col = col.push(self.banner(b));
        }
        col.push(widget::container(widget::scrollable(body)).max_height(PAGE_MAX)).push(ui::inset_divider()).push(lan).into()
    }

    // ---- settings page ------------------------------------------------------------------

    fn toggle_row(&self, t: Toggle, title: String, caption: String, on: bool) -> Element<'_, Message> {
        applet::padded_control(
            Row::new()
                .spacing(space_xs())
                .align_y(Alignment::Center)
                .push(Column::new().width(Length::Fill).push(widget::text::body(title)).push(widget::text::caption(caption).class(Ink::Muted.text())))
                .push(widget::toggler(on).on_toggle(move |v| Message::Set(t, v))),
        )
        .into()
    }

    fn settings_page(&self) -> Element<'_, Message> {
        let copy =
            applet::padded_control(Column::new().spacing(space_xxs()).push(widget::text::body(fl!("set-copy"))).push(
                widget::segmented_control::horizontal(&self.copy_model).button_padding([space_xxxs(), 0, space_xxxs(), 0]).on_activate(Message::CopyMode),
            ));
        Column::new()
            .push(ui::back_header(fl!("settings"), fl!("back"), Message::Page(Page::Main)))
            .push(ui::group_label(fl!("sec-panel")))
            .push(self.toggle_row(Toggle::ShowStatus, fl!("set-status"), fl!("set-status-caption"), self.config.show_status))
            .push(ui::inset_divider())
            .push(ui::group_label(fl!("sec-devices")))
            .push(self.toggle_row(Toggle::ShowOffline, fl!("set-offline"), fl!("set-offline-caption"), self.config.show_offline))
            .push(copy)
            .into()
    }

    #[cfg(feature = "demo")]
    fn show_scene(&mut self, i: usize) {
        let scene = crate::demo::scene(i);
        tracing::info!("demo scene {i}: {}", scene.name);
        self.demo = Some(i);
        self.demo_now = Some(crate::demo::now());
        self.st = scene.state;
        self.toast = None;
    }
}

// ---- small widgets ------------------------------------------------------------------------

/// The `.ftl` string for a key the model hands back.
fn fl_key(key: &str) -> String {
    match key {
        "panel-on" => fl!("panel-on"),
        "panel-off" => fl!("panel-off"),
        "panel-connecting" => fl!("panel-connecting"),
        "panel-error" => fl!("panel-error"),
        "head-connected" => fl!("head-connected"),
        "head-connecting" => fl!("head-connecting"),
        "head-off" => fl!("head-off"),
        "head-login" => fl!("head-login"),
        "head-approval" => fl!("head-approval"),
        "head-other-user" => fl!("head-other-user"),
        "head-unavailable" => fl!("head-unavailable"),
        other => other.to_owned(),
    }
}

/// A `bg-component` box with `radius_s`.
fn boxed(content: Element<'_, Message>) -> Element<'_, Message> {
    widget::container(content)
        .padding([space_xs(), 12])
        .width(Length::Fill)
        .class(theme::Container::custom(|t| widget::container::Style {
            background: Some(Ink::Component.color(t).into()),
            border: cosmic::iced::Border { radius: t.cosmic().corner_radii.radius_s.into(), ..Default::default() },
            ..Default::default()
        }))
        .into()
}

fn copy_button<'a>(text: String) -> Element<'a, Message> {
    widget::button::icon(widget::icon::from_name("edit-copy-symbolic").size(14)).on_press(Message::Copy(text)).tooltip(fl!("copy")).into()
}

/// A notice that ends in a command to run, with a copy button.
fn command_banner<'a>(title: String, body: String, command: String) -> Element<'a, Message> {
    let line = Row::new()
        .spacing(space_xs())
        .align_y(Alignment::Center)
        .push(widget::container(mono(command.clone(), Type::new(12.0, 17.0), Weight::Normal, None)).width(Length::Fill))
        .push(copy_button(command));
    let col = Column::new()
        .spacing(space_xxs())
        .push(widget::text::body(title).font(cosmic::font::semibold()))
        .push(widget::text::caption(body).class(Ink::Muted.text()))
        .push(line);
    widget::container(boxed(col.into())).padding([space_xxs(), space_m()]).into()
}

/// A one-line notice: a warning icon in `ink`, then the text.
fn note<'a>(text: String, ink: Ink) -> Element<'a, Message> {
    let text_ink = if ink == Ink::Muted { Ink::Muted } else { Ink::On };
    widget::container(
        Row::new()
            .spacing(space_xs())
            .push(widget::icon::from_name("dialog-warning-symbolic").size(16).symbolic(true).icon().class(ink.svg()))
            .push(widget::text::caption(text).class(text_ink.text()).width(Length::Fill)),
    )
    .padding([space_xxxs(), space_m()])
    .into()
}

/// "Address   100.64.0.1  [copy]".
fn kv<'a>(key: String, value: String, copy: bool) -> Element<'a, Message> {
    let mut row = Row::new()
        .spacing(space_xs())
        .align_y(Alignment::Center)
        .height(Length::Fixed(28.0))
        .push(widget::text::caption(key).class(Ink::Muted.text()).width(Length::Fixed(KEY_W)))
        .push(widget::text::body(value.clone()).width(Length::Fill).wrapping(cosmic::iced::widget::text::Wrapping::None));
    if copy {
        row = row.push(copy_button(value));
    }
    row.into()
}

/// A radio row; without a message it can't be pressed and, unless `live`, is dimmed.
fn radio<'a>(selected: bool, name: String, caption: Option<(String, Ink)>, live: bool, msg: Option<Message>) -> Element<'a, Message> {
    let mut text = Column::new().push(widget::text::body(name).class(if live { Ink::On } else { Ink::Muted }.text()));
    if let Some((c, ink)) = caption {
        text = text.push(widget::text::caption(c).class(ink.text()));
    }
    let row = Row::new().spacing(space_xs()).align_y(Alignment::Center).push(ui::radio_dot(selected)).push(text.width(Length::Fill));
    applet::menu_button(row).on_press_maybe(msg).into()
}

/// The inverse pill toast.
fn toast<'a>(text: String) -> Element<'a, Message> {
    widget::container(widget::text::caption(text).font(cosmic::font::semibold()).class(cosmic::theme::Text::Custom(|t| cosmic::iced::widget::text::Style {
        color: Some(t.cosmic().background(t.transparent).base.into()),
        ..Default::default()
    })))
    .padding([space_xxxs(), space_s()])
    .class(theme::Container::custom(|t| widget::container::Style {
        background: Some(Ink::On.color(t).into()),
        border: cosmic::iced::Border { radius: t.cosmic().corner_radii.radius_xl.into(), ..Default::default() },
        ..Default::default()
    }))
    .into()
}

/// Opens a sign-in page; only ever an https URL the daemon sent.
fn open_url(url: &str) {
    if !url.starts_with("https://") {
        tracing::warn!("not opening a non-https sign-in URL");
        return;
    }
    if let Err(e) = std::process::Command::new("xdg-open").arg(url).spawn() {
        tracing::warn!("xdg-open: {e}");
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
        let (user, uid) = current_user();
        let mut app = Self {
            core,
            popup: None,
            config,
            handler,
            st: State::new(user, uid),
            fetch_pending: false,
            toast: None,
            toast_id: 0,
            copy_model: segmented_button::Model::builder()
                .insert(|b| b.text(fl!("copy-address")).data(CopyWhat::Address))
                .insert(|b| b.text(fl!("copy-name")).data(CopyWhat::Name))
                .build(),
            #[cfg(feature = "demo")]
            demo: None,
            #[cfg(feature = "demo")]
            demo_now: None,
        };
        app.sync_copy_model();
        if crate::preview_settings() {
            app.st.page = Page::Settings;
        }
        #[cfg(feature = "demo")]
        if crate::demo::enabled() {
            app.show_scene(crate::demo::first());
            if crate::demo::shot_path().is_some() {
                return (
                    app,
                    cosmic::task::future(async {
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        Message::TakeShot
                    }),
                );
            }
        }
        (app, Task::none())
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![self.core.watch_config::<Config>(APP_ID).map(|u| Message::Config(u.config)), escape()];
        if !self.demo() {
            subs.push(Subscription::run(api::watch).map(Message::Bus));
            // The device list is only kept fresh while it's on screen.
            if self.popup.is_some() || preview() {
                subs.push(cosmic::iced::time::every(REFRESH).map(|_| Message::Refresh));
            }
        }
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
                if !self.demo() {
                    self.st.open(Page::Main);
                }
                let open = cosmic::task::message(cosmic::Action::Surface(open_popup(bounds, offset)));
                return Task::batch([open, self.fetch_now()]);
            }
            Message::PopupClosed(id) => {
                if self.popup == Some(id) {
                    self.popup = None;
                }
            }
            Message::Config(c) => {
                if c != self.config {
                    self.config = c;
                    self.sync_copy_model();
                }
            }
            Message::Page(p) => {
                self.st.open(p);
                if p == Page::ExitNode && !self.demo() {
                    return cosmic::task::future(async { Message::Suggested(api::suggest_exit_node().await.ok().flatten()) });
                }
            }
            Message::Escape => {
                if self.st.page != Page::Main {
                    self.st.open(Page::Main);
                } else if let Some(id) = self.popup.take() {
                    return cosmic::task::message(cosmic::Action::Surface(destroy_popup(id)));
                }
            }
            Message::Bus(event) => {
                let url = if let BusEvent::Notify(n) = &event { n.browse_to_url.clone() } else { None };
                let fetch = self.st.apply(event);
                if let Some(url) = url
                    && self.st.should_open(Instant::now())
                {
                    self.st.login_requested = None;
                    open_url(&url);
                }
                if fetch {
                    return self.fetch_soon();
                }
            }
            Message::FetchStatus => {
                self.fetch_pending = false;
                return self.fetch_now();
            }
            Message::Refresh => return self.fetch_now(),
            Message::Status(reply) => match *reply {
                // A reply that raced the daemon going away is stale.
                Ok(status) if self.st.snap.reachable => self.st.snap.status = status,
                Ok(_) => {}
                Err(e) => tracing::debug!("status: {e:?}"),
            },
            Message::Switch(on) => {
                let prefs = self.st.switch(on, Instant::now());
                return self.patch(prefs);
            }
            Message::SelectExit(id) => {
                let prefs = self.st.select_exit(&id, Instant::now());
                return self.patch(prefs);
            }
            Message::AllowLan(on) => {
                self.st.busy = Some((Busy::Exit, Instant::now()));
                return self.patch(MaskedPrefs::allow_lan(on));
            }
            Message::Patched(reply) => {
                self.st.on_patch(reply);
                return self.fetch_soon();
            }
            Message::BusyCheck => {
                if self.st.busy_expired(Instant::now()) {
                    self.st.busy = None;
                    self.st.last_error = Some(String::new());
                }
            }
            Message::Suggested(s) => {
                if self.st.page == Page::ExitNode {
                    self.st.suggestion = s;
                }
            }
            Message::SignIn => {
                self.st.login_requested = Some(Instant::now());
                return cosmic::task::future(async { Message::SignInSent(api::login_interactive().await) });
            }
            Message::SignInSent(reply) => {
                if let Err(e) = reply {
                    self.st.login_requested = None;
                    self.st.on_patch(Err(e));
                }
            }
            Message::Copy(text) => {
                let t = self.toast(fl!("copied", text = text.clone()));
                return Task::batch([cosmic::iced::clipboard::write(text), t]);
            }
            Message::ToastEnd(id) => {
                if self.toast.as_ref().is_some_and(|(_, i)| *i == id) {
                    self.toast = None;
                }
            }
            Message::Query(q) => self.st.query = q,
            Message::ExitQuery(q) => self.st.exit_query = q,
            Message::Set(t, v) => match t {
                Toggle::ShowStatus => self.save(|c, h| c.set_show_status(h, v)),
                Toggle::ShowOffline => self.save(|c, h| c.set_show_offline(h, v)),
            },
            Message::CopyMode(e) => {
                if let Some(v) = self.copy_model.data::<CopyWhat>(e).copied() {
                    self.save(|c, h| c.set_copy(h, v));
                    self.sync_copy_model();
                }
            }
            #[cfg(feature = "demo")]
            Message::DemoNext => {
                let next = self.demo.map_or(0, |i| (i + 1) % crate::demo::SCENES);
                self.show_scene(next);
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
        if preview() {
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
        (!preview()).then(applet::style)
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

#[cfg(test)]
mod tests {
    const SOURCES: [&str; 5] =
        [include_str!("app.rs"), include_str!("widgets/panel.rs"), include_str!("widgets/glyph.rs"), include_str!("model.rs"), include_str!("state.rs")];

    #[test]
    fn no_hex_colours() {
        for src in SOURCES {
            assert!(!src.contains(concat!("Color::from_rgb", "(")), "hard-coded colour");
            assert!(!src.contains(concat!("color", "!(")), "hard-coded colour");
        }
    }

    #[test]
    fn every_string_key_exists() {
        let ftl = include_str!("../i18n/en/cosmic_applet_tailscale.ftl");
        let keys: Vec<&str> = ftl.lines().filter_map(|l| l.split_once(" = ").map(|(k, _)| k.trim())).collect();
        for src in SOURCES {
            for part in src.split(concat!("fl", "!(\"")).skip(1) {
                let key = part.split('"').next().unwrap();
                assert!(keys.contains(&key), "{key} missing from the .ftl file");
            }
        }
        // Keys the model hands to `fl_key`; each must also have an arm there.
        let app = include_str!("app.rs");
        let model = include_str!("model.rs");
        let handed: Vec<&str> = model.split('"').filter(|s| s.starts_with("panel-") || s.starts_with("head-")).collect();
        assert!(handed.len() >= 11);
        for k in handed {
            assert!(keys.contains(&k), "{k} missing from the .ftl file");
            assert!(app.contains(&format!("\"{k}\" => fl",)), "{k} has no arm in fl_key");
        }
    }

    #[test]
    fn every_ftl_key_is_used() {
        let ftl = include_str!("../i18n/en/cosmic_applet_tailscale.ftl");
        let all = SOURCES.concat();
        for key in ftl.lines().filter_map(|l| l.split_once(" = ").map(|(k, _)| k.trim())) {
            assert!(all.contains(&format!("\"{key}\"")), "{key} is in the .ftl file but never used");
        }
    }
}
