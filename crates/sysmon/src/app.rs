//! The applet: state, update, the panel button and the popup.

use std::time::Instant;

use cosmic::app::{Core, Task};
use cosmic::cosmic_config;
use cosmic::iced::alignment::{Horizontal, Vertical};
use cosmic::iced::event::wayland::{Event as WaylandEvent, OutputEvent};
use cosmic::iced::font::Weight;
use cosmic::iced::keyboard::{self, key::Named};
use cosmic::iced::window::Id;
use cosmic::iced::{Alignment, Length, Rectangle, Subscription};
use cosmic::surface::action::{app_popup, destroy_popup};
use cosmic::widget::segmented_button::{self, Entity, SingleSelectModel};
use cosmic::widget::{self, Column, Row};
use cosmic::{Element, applet, theme};

use common::format::{DASH, format_bytes, format_compact, format_ghz, format_gib, format_pct, format_rate, format_size, format_temp, format_w};
use common::graph::{Graph, Grid, layered, shared_max};
use common::ink::Ink;
use common::panel::{Panel, Type};
use common::ring::Ring;
use common::ui::{self, cell, mono, space_m, space_xs, space_xxs, space_xxxs};

use crate::config::{APP_ID, Config, DiskChoice, GpuChoice, PanelStyle};
use crate::fl;
use crate::sample::Sampler;
use crate::widgets::{Meter, Stat, meters, stats, top_row};

/// Room the panel strip and the popup offset take from the output height.
const PANEL_RESERVE: i32 = 80;
/// Below this output height the sections scroll.
const SCROLL_BELOW: i32 = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Main,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Metric {
    Cpu,
    Gpu,
    Ram,
    Disk,
}

impl Metric {
    const ALL: [Self; 4] = [Self::Cpu, Self::Gpu, Self::Ram, Self::Disk];

    fn ink(self) -> Ink {
        match self {
            Metric::Cpu => Ink::Indigo,
            Metric::Gpu => Ink::Purple,
            Metric::Ram => Ink::Green,
            // The R/W keys carry the colour.
            Metric::Disk => Ink::Muted,
        }
    }

    fn label(self) -> String {
        match self {
            Metric::Cpu => fl!("label-cpu"),
            Metric::Gpu => fl!("label-gpu"),
            Metric::Ram => fl!("label-ram"),
            Metric::Disk => fl!("label-disk"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    Tick(Instant),
    Surface(cosmic::surface::Action<Message>),
    TogglePopup(Rectangle, cosmic::iced::Vector),
    PopupClosed(Id),
    Config(Config),
    Page(Page),
    Show(Metric, bool),
    Style(Entity),
    Disk(DiskChoice),
    Gpu(GpuChoice),
    Output(Option<String>, i32),
    Escape,
}

pub struct App {
    core: Core,
    popup: Option<Id>,
    page: Page,
    config: Config,
    handler: Option<cosmic_config::Config>,
    sampler: Sampler,
    style_model: SingleSelectModel,
    /// Logical heights of the outputs, by name.
    outputs: Vec<(String, i32)>,
}

impl App {
    fn gpu_available(&self) -> bool {
        self.sampler.card().is_some()
    }

    fn shown(&self, m: Metric) -> bool {
        match m {
            Metric::Cpu => self.config.show_cpu,
            Metric::Gpu => self.config.show_gpu && self.gpu_available(),
            Metric::Ram => self.config.show_mem,
            Metric::Disk => self.config.show_disk,
        }
    }

    /// Enabled metrics in the fixed order. If the only enabled one is an
    /// unavailable GPU, CPU stands in so the panel is never empty.
    fn metrics(&self) -> Vec<Metric> {
        let v: Vec<Metric> = Metric::ALL.into_iter().filter(|m| self.shown(*m)).collect();
        if v.is_empty() { vec![Metric::Cpu] } else { v }
    }

    fn hot(&self, m: Metric) -> bool {
        match m {
            Metric::Cpu => self.sampler.warn.cpu.0,
            Metric::Gpu => self.sampler.warn.gpu(),
            _ => false,
        }
    }

    fn sync_style_model(&mut self) {
        let want = self.config.style;
        let found = self.style_model.iter().find(|e| self.style_model.data::<PanelStyle>(*e) == Some(&want));
        if let Some(e) = found {
            self.style_model.activate(e);
        }
    }

    fn save<T>(&mut self, set: impl FnOnce(&mut Config, &cosmic_config::Config) -> Result<T, cosmic_config::Error>) {
        if let Some(h) = &self.handler
            && let Err(e) = set(&mut self.config, h)
        {
            tracing::error!("saving config: {e}");
        }
        self.sampler.set_choice(self.config.choice());
    }

    fn tooltip(&self) -> String {
        let l = &self.sampler.latest;
        let pct = |v: Option<f32>| v.map_or_else(|| DASH.to_owned(), |v| format!("{}", v.clamp(0.0, 100.0).round() as u32));
        let mut parts = Vec::new();
        for m in self.metrics() {
            parts.push(match m {
                Metric::Cpu => fl!("a11y-cpu", pct = pct(l.cpu_pct)),
                Metric::Gpu => fl!("a11y-gpu", pct = pct(l.gpu.busy)),
                Metric::Ram => fl!("a11y-ram", pct = pct(l.mem.and_then(|m| m.ram_pct()))),
                Metric::Disk => {
                    let f = |v: Option<f32>| {
                        let r = format_rate(v.map(f64::from));
                        format!("{} {}", r.trimmed(), r.unit)
                    };
                    fl!("a11y-disk", read = f(self.sampler.disk.read.last()), write = f(self.sampler.disk.write.last()))
                }
            });
        }
        if self.sampler.warn.cpu.0 {
            parts.push(fl!("a11y-hot", metric = fl!("cpu"), temp = format_temp(l.tctl)));
        }
        if self.sampler.warn.gpu() {
            let t = if self.sampler.warn.gpu_junction.0 { l.gpu.junction } else { l.gpu.edge };
            parts.push(fl!("a11y-hot", metric = fl!("gpu"), temp = format_temp(t)));
        }
        parts.join(", ")
    }

    // ---- panel ------------------------------------------------------------------------

    fn sparkline(&self, m: Metric, w: f32, h: f32) -> Element<'_, Message> {
        let s = &self.sampler;
        let g = match m {
            Metric::Cpu => Graph { area: Some((&s.cpu, Ink::Indigo)), line: None, max: 100.0, grid: Grid::None, pad: 1.0, stroke: 1.25 },
            Metric::Gpu => Graph { area: Some((&s.gpu, Ink::Purple)), line: None, max: 100.0, grid: Grid::None, pad: 1.0, stroke: 1.25 },
            Metric::Ram => Graph { area: Some((&s.mem, Ink::Green)), line: None, max: 100.0, grid: Grid::None, pad: 1.0, stroke: 1.25 },
            Metric::Disk => Graph {
                area: Some((&s.disk.read, Ink::Blue)),
                line: Some((&s.disk.write, Ink::Orange)),
                max: shared_max(&s.disk.read, &s.disk.write),
                grid: Grid::None,
                pad: 1.0,
                stroke: 1.25,
            },
        };
        layered(g, Length::Fixed(w), Length::Fixed(h))
    }

    /// The value part of a chunk: a 4-character field (DISK: R and W rates).
    fn value(&self, m: Metric, ty: Type, key_ty: Type, vertical: bool) -> Element<'_, Message> {
        let l = &self.sampler.latest;
        let ink = self.hot(m).then_some(Ink::Warning);
        let pct = match m {
            Metric::Cpu => l.cpu_pct,
            Metric::Gpu => l.gpu.busy,
            Metric::Ram => l.mem.and_then(|m| m.ram_pct()),
            Metric::Disk => {
                let key = |k: String, i: Ink| cell(mono(k, key_ty, Weight::Semibold, Some(i)), key_ty.ch(1.0), Horizontal::Left);
                let rate = |v: Option<f32>| cell(mono(format_compact(v.map(f64::from)), ty, Weight::Normal, None), ty.ch(4.0), Horizontal::Right);
                let r = Row::new().align_y(Alignment::Center).push(key(fl!("key-read"), Ink::Blue)).push(rate(self.sampler.disk.read.last()));
                let w = Row::new().align_y(Alignment::Center).push(key(fl!("key-write"), Ink::Orange)).push(rate(self.sampler.disk.write.last()));
                return if vertical { Column::new().push(r).push(w).into() } else { r.push(widget::space().width(Length::Fixed(6.0))).push(w).into() };
            }
        };
        // Centred under the label; the fixed 4-character cell keeps the
        // panel width constant as the value changes.
        cell(mono(format_pct(pct).trim_start().to_owned(), ty, Weight::Normal, ink), ty.ch(4.0), Horizontal::Center)
    }

    fn chunk<'a>(&'a self, m: Metric, p: &Panel, style: PanelStyle) -> Element<'a, Message> {
        let vertical = !p.horizontal;
        let (label_ty, value_ty) = if vertical { (Type::new(9.0, 11.0), Type::new(11.0, 14.0)) } else { (Type::new(10.0, 12.0), Type::new(12.0, 17.0)) };
        let label = mono(m.label(), label_ty, Weight::Semibold, Some(m.ink()));
        let value = || self.value(m, value_ty, label_ty, vertical);
        let align = Alignment::Center;
        let col = |items: Vec<Element<'a, Message>>| -> Element<'a, Message> {
            let mut c = Column::new().align_x(align);
            for i in items {
                c = c.push(i);
            }
            c.into()
        };
        let spark_w = if vertical { p.icon.0 } else { 32.0 };
        match (style, vertical) {
            (PanelStyle::Numbers, _) => col(vec![label.into(), value()]),
            (PanelStyle::Graph, _) => col(vec![
                label.into(),
                widget::space().height(Length::Fixed(2.0)).into(),
                self.sparkline(m, spark_w, if vertical { (spark_w * 0.5).round() } else { 15.0 }),
            ]),
            (PanelStyle::Both, false) => {
                Row::new().spacing(space_xxs()).align_y(Alignment::Center).push(self.sparkline(m, 32.0, 20.0)).push(col(vec![label.into(), value()])).into()
            }
            (PanelStyle::Both, true) => col(vec![label.into(), self.sparkline(m, spark_w, (spark_w * 0.5).round()), value()]),
        }
    }

    fn panel_view(&self) -> Element<'_, Message> {
        let p = Panel::of(&self.core.applet);
        let style = if p.vertical_xs() { PanelStyle::Graph } else { self.config.style };
        let (major, minor) = self.core.applet.suggested_padding(true);
        let (iw, ih) = p.icon;
        let chunks: Vec<Element<'_, Message>> = self.metrics().into_iter().map(|m| self.chunk(m, &p, style)).collect();
        let (content, pad, w, h): (Element<'_, Message>, [u16; 2], Length, Length) = if p.horizontal {
            let mut r = Row::new().spacing(space_xs()).align_y(Alignment::Center);
            for c in chunks {
                r = r.push(c);
            }
            (r.into(), [0, major], Length::Shrink, Length::Fixed(ih + 2.0 * f32::from(minor)))
        } else {
            let mut c = Column::new().spacing(space_xxs()).align_x(Alignment::Center);
            for ch in chunks {
                c = c.push(ch);
            }
            (c.into(), [major, 0], Length::Fixed(iw + 2.0 * f32::from(minor)), Length::Shrink)
        };
        let button = widget::button::custom(widget::container(content).center_x(w).center_y(h))
            .padding(pad)
            .class(theme::Button::AppletIcon)
            .on_press_with_rectangle(|offset, bounds| Message::TogglePopup(bounds, offset));
        let tip = self.core.applet.applet_tooltip::<Message>(button, self.tooltip(), self.popup.is_some(), Message::Surface, None);
        self.core.applet.autosize_window(tip).into()
    }

    // ---- popup ------------------------------------------------------------------------

    /// Height available to the popup on this panel's output.
    fn available(&self) -> Option<i32> {
        let mine = &self.core.applet.output_name;
        self.outputs.iter().find(|(n, _)| n == mine).map(|(_, h)| *h).or_else(|| self.outputs.iter().map(|(_, h)| *h).filter(|h| *h > 0).min())
    }

    fn popup_view(&self) -> Element<'_, Message> {
        let content = match self.page {
            Page::Main => self.main_page(),
            Page::Settings => self.settings_page(),
        };
        let content: Element<'_, Message> = match self.available().filter(|h| *h < SCROLL_BELOW) {
            Some(h) => widget::container(widget::scrollable(content).height(Length::Shrink)).max_height((h - PANEL_RESERVE).max(200) as f32 - 16.0).into(),
            None => content,
        };
        widget::container(content).padding([8, 0, 8, 0]).into()
    }

    fn section<'a>(&self, top: Element<'a, Message>, graph: Element<'a, Message>, details: Vec<Element<'a, Message>>) -> Element<'a, Message> {
        let mut col = Column::new().spacing(space_xxs()).push(top).push(graph);
        for d in details {
            col = col.push(d);
        }
        widget::container(col).padding([space_xxs(), space_m()]).width(Length::Fill).into()
    }

    fn pct_graph<'a>(&'a self, ring: &'a Ring, ink: Ink) -> Element<'a, Message> {
        ui::well(layered(Graph { area: Some((ring, ink)), line: None, max: 100.0, grid: Grid::Half, pad: 4.0, stroke: 1.5 }, Length::Fill, Length::Fixed(64.0)))
    }

    fn headline<'a>(s: String) -> Element<'a, Message> {
        mono(s, Type::new(24.0, 32.0), Weight::Bold, None).into()
    }

    fn hot_key(key: String, hot: bool) -> String {
        if hot { fl!("hot-suffix", key = key) } else { key }
    }

    fn cpu_section(&self) -> Element<'_, Message> {
        let s = &self.sampler;
        let l = &s.latest;
        let caption = fl!("cpu-caption", model = s.info.model.clone(), cores = s.info.cores, threads = s.info.threads);
        let pkg = if s.rapl_denied() { DASH.to_owned() } else { format_w(l.pkg_w) };
        self.section(
            top_row(
                Ink::Indigo,
                fl!("cpu"),
                widget::text::caption(caption).class(Ink::Muted.text()).into(),
                Some(Self::headline(format_pct(l.cpu_pct).trim_start().to_owned())),
            ),
            self.pct_graph(&s.cpu, Ink::Indigo),
            vec![stats(vec![
                Stat { key: fl!("clock-avg"), value: format_ghz(l.cpu_mhz), hot: false },
                Stat { key: Self::hot_key(fl!("tctl"), s.warn.cpu.0), value: format_temp(l.tctl), hot: s.warn.cpu.0 },
                // Without RAPL access the value is a dash; the reason is in
                // the log and the README, never a privilege prompt.
                Stat { key: fl!("package-power"), value: pkg, hot: false },
            ])],
        )
    }

    fn gpu_section(&self) -> Element<'_, Message> {
        let s = &self.sampler;
        let Some(card) = s.card() else {
            return self.section_collapsed(Ink::Purple, fl!("gpu"), fl!("gpu-unavailable"));
        };
        let g = &s.latest.gpu;
        let name = s.gpu_name.as_ref().map_or_else(String::new, |(_, n)| n.clone());
        let mut details = Vec::new();
        let vram = Meter {
            label: fl!("vram"),
            value: format!("{} / {} GiB", format_gib(g.vram_used), format_gib(card.vram_total)),
            frac: g.vram_used.zip(card.vram_total).map(|(u, t)| u as f32 / t as f32),
            ink: if s.warn.vram.0 { Ink::Warning } else { Ink::Purple },
        };
        let mut row = vec![vram];
        match g.cap_w {
            Some(cap) if cap > 0.0 => row.push(Meter {
                label: fl!("power"),
                value: format!("{} / {}", format_w(g.power_w), format_w(Some(cap))),
                frac: g.power_w.map(|w| w / cap),
                ink: Ink::Purple,
            }),
            _ => {}
        }
        details.push(meters(row));
        let (jh, eh) = (s.warn.gpu_junction.0, s.warn.gpu_edge.0);
        let mut st = vec![
            Stat { key: fl!("clock"), value: format_ghz(g.clock_mhz), hot: false },
            Stat { key: Self::hot_key(fl!("edge"), eh), value: format_temp(g.edge), hot: eh },
            Stat { key: Self::hot_key(fl!("junction"), jh), value: format_temp(g.junction), hot: jh },
        ];
        if g.cap_w.is_none() {
            // No power cap: no meter, the watts show as a stat.
            st.push(Stat { key: fl!("power"), value: format_w(g.power_w), hot: false });
        }
        details.push(stats(st));
        self.section(
            top_row(
                Ink::Purple,
                fl!("gpu"),
                widget::text::caption(fl!("gpu-caption", name = name)).class(Ink::Muted.text()).into(),
                Some(Self::headline(format_pct(g.busy).trim_start().to_owned())),
            ),
            self.pct_graph(&s.gpu, Ink::Purple),
            details,
        )
    }

    fn section_collapsed<'a>(&self, ink: Ink, name: String, caption: String) -> Element<'a, Message> {
        widget::container(top_row(ink, name, widget::text::caption(caption).class(Ink::Muted.text()).into(), Some(Self::headline(DASH.to_owned()))))
            .padding([space_xxs(), space_m()])
            .width(Length::Fill)
            .into()
    }

    fn mem_section(&self) -> Element<'_, Message> {
        let s = &self.sampler;
        let m = s.latest.mem;
        let headline: Element<'_, Message> = Row::new()
            .spacing(space_xxxs() + 2)
            .align_y(Alignment::End)
            .push(mono(format_gib(m.map(|m| m.used)), Type::new(24.0, 32.0), Weight::Bold, None))
            .push(widget::container(widget::text::caption(fl!("gib-used")).class(Ink::Muted.text())).padding([0, 0, 5, 0]))
            .into();
        let swap = match m {
            Some(m) if m.swap_total > 0 => Meter {
                label: fl!("swap"),
                value: format!("{} / {} GiB", format_gib(Some(m.swap_used)), format_gib(Some(m.swap_total))),
                frac: Some(m.swap_used as f32 / m.swap_total as f32),
                ink: Ink::Green,
            },
            Some(_) => Meter { label: fl!("swap"), value: fl!("no-swap"), frac: None, ink: Ink::Green },
            None => Meter { label: fl!("swap"), value: DASH.to_owned(), frac: None, ink: Ink::Green },
        };
        let pct = m.and_then(|m| m.ram_pct());
        let ram = Meter {
            label: fl!("ram"),
            value: format_pct(pct).trim_start().to_owned(),
            frac: pct.map(|p| p / 100.0),
            ink: if s.warn.ram.0 { Ink::Warning } else { Ink::Green },
        };
        self.section(
            top_row(
                Ink::Green,
                fl!("memory"),
                widget::text::caption(fl!("mem-caption", total = format_gib(m.map(|m| m.total)))).class(Ink::Muted.text()).into(),
                Some(headline),
            ),
            self.pct_graph(&s.mem, Ink::Green),
            vec![meters(vec![ram, swap])],
        )
    }

    fn disk_section(&self) -> Element<'_, Message> {
        let s = &self.sampler;
        let caption: Element<'_, Message> = match &self.config.disk {
            DiskChoice::Named(d) if s.disk_missing => widget::text::caption(fl!("disk-missing", dev = d.clone())).class(Ink::Warning.text()).into(),
            DiskChoice::Named(d) => {
                let model = s.drives.iter().find(|x| &x.name == d).map_or_else(String::new, |x| x.model.clone());
                widget::text::caption(format!("{d} · {model}")).class(Ink::Muted.text()).into()
            }
            DiskChoice::All => widget::text::caption(fl!("all-drives")).class(Ink::Muted.text()).into(),
        };
        let (read, write) = (&s.disk.read, &s.disk.write);
        let readouts: Element<'_, Message> = Row::new()
            .spacing(space_xs())
            .push(ui::rate_readout(fl!("read"), Ink::Blue, true, &format_rate(read.last().map(f64::from))))
            .push(ui::rate_readout(fl!("write"), Ink::Orange, false, &format_rate(write.last().map(f64::from))))
            .into();
        let max = shared_max(read, write);
        let scale = format_rate(Some(f64::from(max)));
        let graph = ui::well(
            cosmic::iced::widget::stack([
                layered(
                    Graph { area: Some((read, Ink::Blue)), line: Some((write, Ink::Orange)), max, grid: Grid::Half, pad: 4.0, stroke: 1.5 },
                    Length::Fill,
                    Length::Fixed(64.0),
                ),
                widget::container(mono(format!("{} {}", scale.trimmed(), scale.unit), Type::new(12.0, 17.0), Weight::Normal, Some(Ink::Muted)))
                    .padding([space_xxxs(), space_xxs()])
                    .align_x(Horizontal::Right)
                    .align_y(Vertical::Top)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into(),
            ])
            .width(Length::Fill)
            .height(Length::Fixed(64.0)),
        );
        let (tr, tw) = s.disk.totals();
        let bytes = |b: u64| {
            let f = format_bytes(Some(b as f64));
            format!("{}\u{a0}{}", f.trimmed(), f.unit)
        };
        let nh = s.warn.nvme.0;
        let top = top_row(Ink::Muted, fl!("disk-io"), caption, None);
        let mut col = Column::new().spacing(space_xxs()).push(top).push(readouts).push(graph);
        col = col.push(stats(vec![
            Stat { key: fl!("read-since-login"), value: bytes(tr), hot: false },
            Stat { key: fl!("written"), value: bytes(tw), hot: false },
            Stat { key: Self::hot_key(fl!("nvme-temp"), nh), value: format_temp(s.latest.nvme_temp), hot: nh },
        ]));
        widget::container(col).padding([space_xxs(), space_m()]).width(Length::Fill).into()
    }

    fn main_page(&self) -> Element<'_, Message> {
        Column::new()
            .push(self.cpu_section())
            .push(ui::inset_divider())
            .push(self.gpu_section())
            .push(ui::inset_divider())
            .push(self.mem_section())
            .push(ui::inset_divider())
            .push(self.disk_section())
            .push(ui::inset_divider())
            .push(ui::settings_row(fl!("applet-settings"), Message::Page(Page::Settings)))
            .into()
    }

    fn settings_page(&self) -> Element<'_, Message> {
        let on: Vec<Metric> = Metric::ALL.into_iter().filter(|m| self.shown(*m)).collect();
        let mut toggles = Column::new();
        for m in Metric::ALL {
            let (name, caption, ink) = match m {
                Metric::Cpu => (fl!("cpu"), fl!("usage-pct"), Ink::Indigo),
                Metric::Gpu => (fl!("gpu"), fl!("usage-pct"), Ink::Purple),
                Metric::Ram => (fl!("memory"), fl!("ram-used-pct"), Ink::Green),
                Metric::Disk => (fl!("disk-io"), fl!("read-write"), Ink::Muted),
            };
            let checked = self.shown(m);
            let last = checked && on.len() == 1;
            let unavailable = m == Metric::Gpu && !self.gpu_available();
            let caption = if unavailable {
                fl!("gpu-unavailable")
            } else if last {
                fl!("at-least-one")
            } else {
                caption
            };
            let toggler = widget::toggler(checked).on_toggle_maybe((!last && !unavailable).then_some(move |v| Message::Show(m, v)));
            toggles = toggles.push(applet::padded_control(
                Row::new()
                    .spacing(space_xs())
                    .align_y(Alignment::Center)
                    .push(ui::dot(ink))
                    .push(Column::new().width(Length::Fill).push(widget::text::body(name)).push(widget::text::caption(caption).class(Ink::Muted.text())))
                    .push(toggler),
            ));
        }

        let s = &self.sampler;
        let mut disks = Column::new().push(ui::radio_row(
            self.config.disk == DiskChoice::All,
            None,
            fl!("all-drives"),
            Some((fl!("all-drives-caption"), Ink::Muted)),
            Message::Disk(DiskChoice::All),
        ));
        for d in &s.drives {
            disks = disks.push(ui::radio_row(
                self.config.disk == DiskChoice::Named(d.name.clone()),
                None,
                d.name.clone(),
                Some((fl!("drive-caption", model = d.model.clone(), size = format_size(d.size)), Ink::Muted)),
                Message::Disk(DiskChoice::Named(d.name.clone())),
            ));
        }

        let mut col = Column::new()
            .push(ui::back_header(fl!("applet-settings"), fl!("back"), Message::Page(Page::Main)))
            .push(ui::group_label(fl!("show-in-panel")))
            .push(toggles)
            .push(ui::inset_divider())
            .push(ui::group_label(fl!("panel-style")))
            .push(applet::padded_control(
                widget::segmented_control::horizontal(&self.style_model).button_padding([space_xxxs(), 0, space_xxxs(), 0]).on_activate(Message::Style),
            ))
            .push(ui::inset_divider())
            .push(ui::group_label(fl!("disk")))
            .push(disks);
        if s.cards.len() > 1 {
            let mut gpus = Column::new();
            let shown = s.card().map(|c| c.slot.clone());
            for c in &s.cards {
                let name = if s.gpu_name.as_ref().is_some_and(|(slot, _)| slot == &c.slot) {
                    s.gpu_name.as_ref().map(|(_, n)| n.clone()).unwrap_or_default()
                } else {
                    format!("AMD GPU ({:04x}:{:04x})", c.ids.0, c.ids.1)
                };
                gpus = gpus.push(ui::radio_row(
                    shown.as_deref() == Some(c.slot.as_str()),
                    None,
                    name,
                    Some((c.slot.clone(), Ink::Muted)),
                    Message::Gpu(GpuChoice::Slot(c.slot.clone())),
                ));
            }
            col = col.push(ui::inset_divider()).push(ui::group_label(fl!("gpu-picker"))).push(gpus);
        }
        col.into()
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
        let style_model = segmented_button::Model::builder()
            .insert(|b| b.text(fl!("style-numbers")).data(PanelStyle::Numbers).activate())
            .insert(|b| b.text(fl!("style-graph")).data(PanelStyle::Graph))
            .insert(|b| b.text(fl!("style-both")).data(PanelStyle::Both))
            .build();
        let mut sampler = Sampler::new(config.choice());
        sampler.tick(Instant::now(), crate::preview());
        let mut app = Self {
            core,
            popup: None,
            page: if crate::preview_settings() { Page::Settings } else { Page::Main },
            config,
            handler,
            sampler,
            style_model,
            outputs: Vec::new(),
        };
        app.sync_style_model();
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
                cosmic::iced::Event::PlatformSpecific(cosmic::iced::event::PlatformSpecific::Wayland(WaylandEvent::Output(
                    OutputEvent::Created(Some(info)) | OutputEvent::InfoUpdate(info),
                    _,
                ))) => Some(Message::Output(info.name.clone(), info.logical_size.map_or(0, |s| s.1))),
                _ => None,
            }),
        ])
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick(now) => {
                self.sampler.tick(now, self.popup.is_some() || crate::preview());
                let l = &self.sampler.latest;
                tracing::debug!(cpu = ?l.cpu_pct, gpu = ?l.gpu.busy, tctl = ?l.tctl, ram = ?l.mem.and_then(|m| m.ram_pct()), read = ?self.sampler.disk.read.last(), write = ?self.sampler.disk.write.last(), "sample");
            }
            Message::Surface(a) => return cosmic::task::message(cosmic::Action::Surface(a)),
            Message::TogglePopup(bounds, offset) => {
                if let Some(id) = self.popup.take() {
                    return cosmic::task::message(cosmic::Action::Surface(destroy_popup(id)));
                }
                self.page = Page::Main;
                self.sampler.discover();
                self.sampler.read_extras();
                return cosmic::task::message(cosmic::Action::Surface(open_popup(bounds, offset)));
            }
            Message::PopupClosed(id) => {
                if self.popup == Some(id) {
                    self.popup = None;
                }
            }
            Message::Config(c) => {
                let c = c.enforce();
                if c != self.config {
                    self.config = c;
                    self.sampler.set_choice(self.config.choice());
                    self.sync_style_model();
                }
            }
            Message::Page(p) => self.page = p,
            Message::Show(m, v) => {
                // The last enabled metric can't be turned off.
                let others = Metric::ALL.into_iter().filter(|x| *x != m && self.shown(*x)).count();
                if v || others > 0 {
                    match m {
                        Metric::Cpu => self.save(|c, h| c.set_show_cpu(h, v)),
                        Metric::Gpu => self.save(|c, h| c.set_show_gpu(h, v)),
                        Metric::Ram => self.save(|c, h| c.set_show_mem(h, v)),
                        Metric::Disk => self.save(|c, h| c.set_show_disk(h, v)),
                    }
                }
            }
            Message::Style(e) => {
                self.style_model.activate(e);
                if let Some(st) = self.style_model.data::<PanelStyle>(e).copied() {
                    self.save(|c, h| c.set_style(h, st));
                }
            }
            Message::Disk(d) => self.save(|c, h| c.set_disk(h, d)),
            Message::Gpu(g) => self.save(|c, h| c.set_gpu(h, g)),
            Message::Output(name, h) => {
                let name = name.unwrap_or_default();
                match self.outputs.iter_mut().find(|(n, _)| *n == name) {
                    Some(o) => o.1 = h,
                    None => self.outputs.push((name, h)),
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
