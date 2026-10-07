//! The applet: state, update, the panel button and the popup.

use std::time::Instant;

use cosmic::app::{Core, Task};
use cosmic::cosmic_config;
use cosmic::iced::alignment::{Horizontal, Vertical};
use cosmic::iced::font::Weight;
use cosmic::iced::keyboard::{self, key::Named};
use cosmic::iced::window::Id;
use cosmic::iced::{Alignment, Length, Rectangle, Subscription};
use cosmic::surface::action::{app_popup, destroy_popup};
use cosmic::widget::segmented_button::{self, Entity, SingleSelectModel};
use cosmic::widget::{self, Column, Row};
use cosmic::{Element, applet, theme};

use common::format::format_rate;
use common::graph::{Graph, Grid, layered, shared_max};
use common::ink::Ink;
use common::panel::{Panel, SizeClass, Type};
use common::ring::Ring;
use common::ui::{self, mono, space_xs, space_xxs, space_xxxs};

use crate::config::{APP_ID, AdapterChoice, Config, DisplayMode, Indicator};
use crate::fl;
use crate::net::sampler::Sampler;
use crate::net::{Iface, Kind, ordered};
use crate::widgets::picker::picker;
use crate::widgets::rates::{self, Form, rates};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Main,
    Settings,
}

#[derive(Debug, Clone)]
pub enum Message {
    Tick(Instant),
    Surface(cosmic::surface::Action<Message>),
    TogglePopup(Rectangle, cosmic::iced::Vector),
    PopupClosed(Id),
    Config(Config),
    Page(Page),
    Mode(Entity),
    Indicator(Indicator),
    Adapter(AdapterChoice),
    NetworkSettings,
    Escape,
}

/// Why the shown interface has no live values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    CableUnplugged,
    InterfaceDown,
    ReadError,
}

/// The interface on show and its state.
pub struct Shown<'a> {
    pub iface: Option<&'a Iface>,
    pub automatic: bool,
    /// `Named(x)` that isn't present.
    pub missing: Option<&'a str>,
    pub problem: Option<Reason>,
}

pub struct App {
    core: Core,
    popup: Option<Id>,
    page: Page,
    config: Config,
    handler: Option<cosmic_config::Config>,
    sampler: Sampler,
    mode_model: SingleSelectModel,
    /// Empty history for "no interface".
    empty: Ring,
}

impl App {
    fn shown(&self) -> Shown<'_> {
        let s = &self.sampler;
        let (named, missing) = match &self.config.adapter {
            AdapterChoice::Named(n) => match s.iface(n) {
                Some(i) => (Some(i), None),
                None => (None, Some(n.as_str())),
            },
            AdapterChoice::Automatic => (None, None),
        };
        let automatic = named.is_none();
        let iface = named.or_else(|| {
            // The default route, else the first physical interface that is
            // up, else (to show as disconnected) the first physical one.
            let list = ordered(&s.ifaces);
            s.default_route
                .as_deref()
                .and_then(|r| s.iface(r))
                .or_else(|| list.iter().copied().find(|i| i.kind.physical() && i.up))
                .or_else(|| list.iter().copied().find(|i| i.kind.physical()))
                .or_else(|| list.first().copied())
        });
        let problem = iface.and_then(|i| {
            if s.series.get(&i.name).is_some_and(|x| x.fails >= 2) {
                Some(Reason::ReadError)
            } else if !i.up {
                Some(Reason::InterfaceDown)
            } else if !i.carrier {
                Some(Reason::CableUnplugged)
            } else {
                None
            }
        });
        Shown { iface, automatic, missing, problem }
    }

    fn histories(&self, name: Option<&str>) -> (&Ring, &Ring) {
        match name.and_then(|n| self.sampler.series.get(n)) {
            Some(s) => (&s.down, &s.up),
            None => (&self.empty, &self.empty),
        }
    }

    fn sync_mode_model(&mut self) {
        let want = self.config.mode;
        let found = self.mode_model.iter().find(|e| self.mode_model.data::<DisplayMode>(*e) == Some(&want));
        if let Some(e) = found {
            self.mode_model.activate(e);
        }
    }

    fn save<T>(&mut self, set: impl FnOnce(&mut Config, &cosmic_config::Config) -> Result<T, cosmic_config::Error>) {
        if let Some(h) = &self.handler
            && let Err(e) = set(&mut self.config, h)
        {
            tracing::error!("saving config: {e}");
        }
    }

    fn tooltip(&self) -> String {
        let shown = self.shown();
        let name = shown.iface.map_or_else(|| fl!("adapter-automatic"), |i| i.name.clone());
        if shown.problem.is_some() || shown.iface.is_none() {
            return fl!("a11y-disconnected", iface = name);
        }
        let (d, u) = self.histories(shown.iface.map(|i| i.name.as_str()));
        let f = |v: Option<f32>| {
            let r = format_rate(v.map(f64::from));
            format!("{} {}", r.trimmed(), r.unit)
        };
        fl!("a11y-summary", down = f(d.last()), up = f(u.last()), iface = name)
    }

    // ---- panel ------------------------------------------------------------------------

    fn panel_view(&self) -> Element<'_, Message> {
        let p = Panel::of(&self.core.applet);
        let shown = self.shown();
        let (down, up) = self.histories(shown.iface.map(|i| i.name.as_str()));
        let off = shown.iface.is_none() || shown.problem.is_some();
        let mode = if p.vertical_xs() { DisplayMode::Graph } else { self.config.mode };
        let (major, minor) = self.core.applet.suggested_padding(true);
        let (iw, ih) = p.icon;

        let graph = || {
            let (w, h) = if p.horizontal { ((ih * 2.4).round(), ih) } else { (iw, iw) };
            layered(
                Graph { area: Some((down, Ink::Blue)), line: Some((up, Ink::Orange)), max: shared_max(down, up), grid: Grid::None, pad: 1.0, stroke: 1.25 },
                Length::Fixed(w),
                Length::Fixed(h),
            )
        };
        let numbers = || {
            let thickness = if p.horizontal { ih } else { iw } + 2.0 * f32::from(minor);
            let (ty, icon_px, rxtx_px, form) = match (p.horizontal, p.class) {
                (true, SizeClass::XS | SizeClass::S) => (Type::new(12.0, 17.0), 12.0, 10.0, Form::Rate),
                (true, _) => (Type::new(14.0, 20.0), 14.0, 10.0, Form::Rate),
                (false, _) => (Type::new(11.0, 14.0), 10.0, 9.0, Form::Compact),
            };
            // Two lines must fit the panel's thickness (XS is 32 px).
            let ty = Type::new(ty.size, ty.line.min((thickness / 2.0).floor()));
            rates(
                rates::Style {
                    form,
                    indicator: self.config.indicator,
                    ty,
                    icon_px,
                    rxtx_px,
                    gap: if p.horizontal { f32::from(space_xxxs()) } else { 0.0 },
                    number_ink: None,
                    off,
                },
                down.last().map(f64::from),
                up.last().map(f64::from),
            )
        };
        let content: Element<'_, Message> = match (mode, p.horizontal) {
            (DisplayMode::Numbers, _) => numbers(),
            (DisplayMode::Graph, _) => graph(),
            (DisplayMode::Both, true) => Row::new().spacing(space_xxs()).align_y(Alignment::Center).push(graph()).push(numbers()).into(),
            (DisplayMode::Both, false) => Column::new().spacing(space_xxs()).align_x(Alignment::Center).push(graph()).push(numbers()).into(),
        };

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

    fn state_caption(&self, shown: &Shown<'_>) -> Element<'_, Message> {
        let Some(i) = shown.iface else {
            return widget::text::caption(fl!("adapter-automatic-none")).class(Ink::Muted.text()).into();
        };
        let mut parts: Vec<String> = Vec::new();
        if shown.automatic {
            parts.push(fl!("adapter-automatic"));
        }
        parts.push(match i.kind {
            Kind::Ethernet => fl!("type-ethernet"),
            Kind::Wifi => fl!("type-wifi"),
            Kind::Virtual => fl!("type-virtual"),
        });
        if let Some(s) = i.speed.filter(|_| shown.problem.is_none()) {
            parts.push(speed(s));
        }
        let (state, ink) = match (shown.problem, i.kind) {
            (Some(_), _) => (fl!("state-disconnected"), Ink::Destructive),
            (None, Kind::Virtual) => (fl!("state-up"), Ink::Success),
            (None, _) => (fl!("state-connected"), Ink::Success),
        };
        let caption = Row::new()
            .push(widget::text::caption(format!("{} · ", parts.join(" · "))).class(Ink::Muted.text()))
            .push(widget::text::caption(state).class(ink.text()))
            .wrap();
        let mut col = Column::new().push(caption);
        if let Some(x) = shown.missing {
            col = col.push(widget::text::caption(format!("{} · {}", fl!("adapter-missing", iface = x), fl!("using-automatic"))).class(Ink::Warning.text()));
        }
        col.into()
    }

    fn main_page(&self) -> Element<'_, Message> {
        let shown = self.shown();
        let name = shown.iface.map(|i| i.name.as_str());
        let (down, up) = self.histories(name);
        let off = shown.iface.is_none() || shown.problem.is_some();

        let header = applet::padded_control(
            Row::new()
                .spacing(space_xs())
                .align_y(Alignment::Center)
                .push(widget::icon::from_name(shown.iface.map_or(Kind::Ethernet, |i| i.kind).icon()).size(20).symbolic(true))
                .push(Column::new().push(widget::text::heading(name.map_or_else(|| fl!("adapter-automatic"), str::to_owned))).push(self.state_caption(&shown))),
        );

        let value = |v: Option<f32>| format_rate(if off { None } else { v.map(f64::from) });
        let readouts = applet::padded_control(
            Row::new().spacing(space_xs()).push(ui::rate_readout(fl!("download"), Ink::Blue, true, &value(down.last()))).push(ui::rate_readout(
                fl!("upload"),
                Ink::Orange,
                false,
                &value(up.last()),
            )),
        );

        let max = shared_max(down, up);
        let scale = format_rate(Some(f64::from(max)));
        let span = match shown.problem {
            Some(Reason::CableUnplugged) => fl!("reason-cable-unplugged"),
            Some(Reason::InterfaceDown) => fl!("reason-interface-down"),
            Some(Reason::ReadError) => fl!("reason-read-error"),
            None => fl!("graph-span"),
        };
        let pad = f32::from(space_xxxs());
        let graph = cosmic::iced::widget::stack([
            layered(
                Graph { area: Some((down, Ink::Blue)), line: Some((up, Ink::Orange)), max, grid: Grid::Quarters, pad, stroke: 1.5 },
                Length::Fill,
                Length::Fixed(120.0),
            ),
            widget::container(mono(format!("{} {}", scale.trimmed(), scale.unit), Type::new(12.0, 17.0), Weight::Normal, Some(Ink::Muted)))
                .padding([space_xxxs(), space_xxs()])
                .align_x(Horizontal::Right)
                .align_y(Vertical::Top)
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
            widget::container(widget::text::caption(span).class(if shown.problem.is_some() { Ink::Destructive } else { Ink::Muted }.text()))
                .padding([space_xxxs(), space_xxs()])
                .align_x(Horizontal::Left)
                .align_y(Vertical::Bottom)
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
        ])
        .width(Length::Fill)
        .height(Length::Fixed(120.0));
        let graph = applet::padded_control(ui::well(graph));

        let (rx, tx) = name.and_then(|n| self.sampler.series.get(n)).map_or((None, None), |s| {
            let (r, t) = s.totals();
            (Some(r as f64), Some(t as f64))
        });
        let totals = applet::padded_control(
            Row::new().align_y(Alignment::Center).push(widget::text::caption(fl!("since-login")).class(Ink::Muted.text()).width(Length::Fill)).push(rates(
                rates::Style {
                    form: Form::Bytes,
                    indicator: self.config.indicator,
                    ty: Type::new(14.0, 20.0),
                    icon_px: 14.0,
                    rxtx_px: 10.0,
                    gap: f32::from(space_xxxs()),
                    number_ink: Some(Ink::Muted),
                    off: false,
                },
                rx,
                tx,
            )),
        );

        Column::new()
            .push(header)
            .push(readouts)
            .push(graph)
            .push(totals)
            .push(ui::inset_divider())
            .push(ui::settings_row(fl!("applet-settings"), Message::Page(Page::Settings)))
            .push(applet::menu_button(widget::text::body(fl!("network-settings"))).on_press(Message::NetworkSettings))
            .into()
    }

    fn settings_page(&self) -> Element<'_, Message> {
        let mode = widget::segmented_control::horizontal(&self.mode_model).button_padding([space_xxxs(), 0, space_xxxs(), 0]).on_activate(Message::Mode);
        let (name, note) = indicator_text(self.config.indicator);

        let auto_caption = match self.sampler.default_route.as_deref() {
            Some(r) => fl!("adapter-automatic-caption", iface = r),
            None => fl!("adapter-automatic-none"),
        };
        let mut adapters = Column::new().push(ui::radio_row(
            self.config.adapter == AdapterChoice::Automatic,
            None,
            fl!("adapter-automatic"),
            Some((auto_caption, Ink::Muted)),
            Message::Adapter(AdapterChoice::Automatic),
        ));
        let selected_name = match &self.config.adapter {
            AdapterChoice::Named(n) => Some(n.as_str()),
            AdapterChoice::Automatic => None,
        };
        for i in ordered(&self.sampler.ifaces) {
            adapters = adapters.push(ui::radio_row(
                selected_name == Some(i.name.as_str()),
                Some(i.kind.icon()),
                i.name.clone(),
                Some(iface_caption(i)),
                Message::Adapter(AdapterChoice::Named(i.name.clone())),
            ));
        }
        // A selected adapter that's gone stays listed, as disconnected.
        if let Some(n) = selected_name.filter(|n| self.sampler.iface(n).is_none()) {
            adapters = adapters.push(ui::radio_row(
                true,
                None,
                n.to_owned(),
                Some((fl!("state-disconnected"), Ink::Destructive)),
                Message::Adapter(AdapterChoice::Named(n.to_owned())),
            ));
        }

        Column::new()
            .push(ui::back_header(fl!("applet-settings"), fl!("back"), Message::Page(Page::Main)))
            .push(ui::group_label(fl!("show-in-panel")))
            .push(applet::padded_control(mode))
            .push(ui::inset_divider())
            .push(ui::group_label(fl!("direction-indicator")))
            .push(applet::padded_control(
                Column::new()
                    .spacing(space_xxs())
                    .push(picker(self.config.indicator, Message::Indicator))
                    .push(widget::text::caption(format!("{name} · {note}")).class(Ink::Muted.text())),
            ))
            .push(ui::inset_divider())
            .push(ui::group_label(fl!("adapter")))
            .push(adapters)
            .into()
    }
}

fn speed(mbps: u32) -> String {
    if mbps >= 1000 {
        let g = f64::from(mbps) / 1000.0;
        let s = if mbps.is_multiple_of(1000) { format!("{}", mbps / 1000) } else { format!("{g:.1}") };
        fl!("speed-gbps", speed = s)
    } else {
        fl!("speed-mbps", speed = mbps.to_string())
    }
}

fn iface_caption(i: &Iface) -> (String, Ink) {
    let kind = match i.kind {
        Kind::Ethernet => fl!("type-ethernet"),
        Kind::Wifi => fl!("type-wifi"),
        Kind::Virtual => fl!("type-virtual"),
    };
    if i.up && i.carrier {
        let state = if i.kind == Kind::Virtual { fl!("state-up") } else { fl!("state-connected") };
        let mut parts = vec![kind];
        if let Some(s) = i.speed {
            parts.push(speed(s));
        }
        parts.push(state);
        (parts.join(" · "), Ink::Muted)
    } else {
        (format!("{kind} · {}", fl!("state-disconnected")), Ink::Destructive)
    }
}

fn indicator_text(i: Indicator) -> (String, String) {
    match i {
        Indicator::Arrows => (fl!("indicator-arrows"), fl!("indicator-note-arrows")),
        Indicator::Triangles => (fl!("indicator-triangles"), fl!("indicator-note-triangles")),
        Indicator::Chevrons => (fl!("indicator-chevrons"), fl!("indicator-note-chevrons")),
        Indicator::RxTxIcons => (fl!("indicator-rxtx-icons"), fl!("indicator-note-rxtx-icons")),
        Indicator::Bar => (fl!("indicator-bar"), fl!("indicator-note-bar")),
        Indicator::RxTx => (fl!("indicator-rxtx"), fl!("indicator-note-rxtx")),
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
        let mut mode_model = segmented_button::Model::builder()
            .insert(|b| b.text(fl!("mode-numbers")).data(DisplayMode::Numbers))
            .insert(|b| b.text(fl!("mode-graph")).data(DisplayMode::Graph))
            .insert(|b| b.text(fl!("mode-both")).data(DisplayMode::Both))
            .build();
        mode_model.activate_position(2);
        let mut sampler = Sampler::default();
        sampler.tick(Instant::now(), None);
        let mut app = Self {
            core,
            popup: None,
            page: if crate::preview_settings() { Page::Settings } else { Page::Main },
            config,
            handler,
            sampler,
            mode_model,
            empty: Ring::default(),
        };
        app.sync_mode_model();
        (app, Task::none())
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            cosmic::iced::time::every(std::time::Duration::from_secs(1)).map(Message::Tick),
            self.core.watch_config::<Config>(APP_ID).map(|u| Message::Config(u.config)),
            cosmic::iced::event::listen_with(|event, _, _| match event {
                cosmic::iced::Event::Keyboard(keyboard::Event::KeyPressed { key: keyboard::Key::Named(Named::Escape), .. }) => Some(Message::Escape),
                _ => None,
            }),
        ])
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick(now) => {
                let watch = self.shown().iface.map(|i| i.name.clone());
                self.sampler.tick(now, watch.as_deref());
                if tracing::enabled!(tracing::Level::DEBUG)
                    && let Some(n) = &watch
                    && let Some((d, u)) = self.sampler.series.get(n).map(|s| s.last())
                {
                    tracing::debug!(iface = %n, down = ?d, up = ?u, "rates");
                }
            }
            Message::Surface(a) => return cosmic::task::message(cosmic::Action::Surface(a)),
            Message::TogglePopup(bounds, offset) => {
                if let Some(id) = self.popup.take() {
                    return cosmic::task::message(cosmic::Action::Surface(destroy_popup(id)));
                }
                self.page = Page::Main;
                // Pick up hot-plugged adapters.
                self.sampler.discover();
                return cosmic::task::message(cosmic::Action::Surface(open_popup(bounds, offset)));
            }
            Message::PopupClosed(id) => {
                if self.popup == Some(id) {
                    self.popup = None;
                }
            }
            Message::Config(c) => {
                if c != self.config {
                    self.config = c;
                    self.sync_mode_model();
                }
            }
            Message::Page(p) => self.page = p,
            Message::Mode(e) => {
                self.mode_model.activate(e);
                if let Some(m) = self.mode_model.data::<DisplayMode>(e).copied() {
                    self.save(|c, h| c.set_mode(h, m));
                }
            }
            Message::Indicator(i) => self.save(|c, h| c.set_indicator(h, i)),
            Message::Adapter(a) => self.save(|c, h| c.set_adapter(h, a)),
            Message::NetworkSettings => {
                // Waited on from a thread so it never lingers as a zombie.
                std::thread::spawn(|| match std::process::Command::new("cosmic-settings").arg("network").spawn() {
                    Ok(mut child) => {
                        let _ = child.wait();
                    }
                    Err(e) => tracing::warn!("launching cosmic-settings: {e}"),
                });
                if let Some(id) = self.popup.take() {
                    return cosmic::task::message(cosmic::Action::Surface(destroy_popup(id)));
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
    #[test]
    fn speeds() {
        assert_eq!(super::speed(1000), "1 Gb/s");
        assert_eq!(super::speed(2500), "2.5 Gb/s");
        assert_eq!(super::speed(100), "100 Mb/s");
    }

    #[test]
    fn no_hex_colours() {
        // ACCEPTANCE C-02: colours come from the theme.
        for src in [include_str!("app.rs"), include_str!("widgets/rates.rs"), include_str!("widgets/picker.rs")] {
            assert!(!src.contains(concat!("Color::from_rgb", "(")), "hard-coded colour");
            assert!(!src.contains(concat!("#", "63d0df")));
        }
    }
}
