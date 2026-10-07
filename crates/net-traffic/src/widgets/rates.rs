//! A pair of rate lines, each a row of three fixed-width cells:
//! indicator | number (4 ch, right) | unit (4 ch, left). The indicator is
//! never part of the number's string, so the two always line up.

use cosmic::Element;
use cosmic::iced::alignment::Horizontal;
use cosmic::iced::font::Weight;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{self, Column, Row};

use common::format::{DASH, format_bytes, format_compact, format_rate};
use common::ink::Ink;
use common::panel::Type;
use common::ui::{cell, mono};

use crate::config::Indicator;

const BAR_DOWN: &[u8] = include_bytes!("../../resources/icons/hicolor/scalable/actions/net-down-bar-symbolic.svg");
const BAR_UP: &[u8] = include_bytes!("../../resources/icons/hicolor/scalable/actions/net-up-bar-symbolic.svg");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    /// `860 kB/s`
    Rate,
    /// `860K`, no unit cell.
    Compact,
    /// `4.21 GB`
    Bytes,
}

#[derive(Debug, Clone, Copy)]
pub struct Style {
    pub form: Form,
    pub indicator: Indicator,
    pub ty: Type,
    /// Symbolic icon size.
    pub icon_px: f32,
    /// RX/TX text size.
    pub rxtx_px: f32,
    /// Gap between indicator and number.
    pub gap: f32,
    pub number_ink: Option<Ink>,
    /// Disconnected: indicators and dashes in destructive.
    pub off: bool,
}

/// One direction's indicator glyph or icon.
pub fn indicator<'a, M: 'a>(ind: Indicator, down: bool, icon_px: f32, text_px: f32, rxtx_px: f32, ink: Ink) -> Element<'a, M> {
    let named = |d: &'static str, u: &'static str| -> Element<'a, M> {
        widget::icon(widget::icon::from_name(if down { d } else { u }).symbolic(true).handle()).size(icon_px as u16).class(ink.svg()).into()
    };
    match ind {
        Indicator::Arrows => widget::text(if down { "↓" } else { "↑" }).size(text_px).class(ink.text()).into(),
        Indicator::Triangles => named("pan-down-symbolic", "pan-up-symbolic"),
        Indicator::Chevrons => named("go-down-symbolic", "go-up-symbolic"),
        Indicator::RxTxIcons => named("network-receive-symbolic", "network-transmit-symbolic"),
        Indicator::Bar => {
            widget::icon(widget::icon::from_svg_bytes(if down { BAR_DOWN } else { BAR_UP }).symbolic(true)).size(icon_px as u16).class(ink.svg()).into()
        }
        Indicator::RxTx => mono(if down { "RX" } else { "TX" }, Type::new(rxtx_px, rxtx_px + 2.0), Weight::Semibold, Some(ink)).into(),
    }
}

fn line<'a, M: 'a>(s: &Style, down: bool, v: Option<f64>) -> Element<'a, M> {
    let ink = if s.off {
        Ink::Destructive
    } else if down {
        Ink::Blue
    } else {
        Ink::Orange
    };
    let ind_w = if s.indicator == Indicator::RxTx { Type::new(s.rxtx_px, 0.0).ch(2.4) } else { s.icon_px.max(s.ty.size) };
    let (num, unit) = match (s.off, s.form) {
        (true, _) => (DASH.to_owned(), ""),
        (false, Form::Rate) => {
            let r = format_rate(v);
            (r.trimmed().to_owned(), r.unit)
        }
        (false, Form::Bytes) => {
            let r = format_bytes(v);
            (r.trimmed().to_owned(), r.unit)
        }
        (false, Form::Compact) => (format_compact(v), ""),
    };
    let num_ink = if s.off { Some(Ink::Destructive) } else { s.number_ink };
    let mut row = Row::new()
        .align_y(Alignment::Center)
        .push(cell(indicator(s.indicator, down, s.icon_px, s.ty.size, s.rxtx_px, ink), ind_w, Horizontal::Center))
        .push(widget::space().width(Length::Fixed(s.gap)))
        .push(cell(mono(num, s.ty, Weight::Normal, num_ink), s.ty.ch(4.0), Horizontal::Right));
    if s.form != Form::Compact {
        row = row.push(widget::space().width(Length::Fixed(s.ty.ch(1.0)))).push(cell(
            mono(unit, s.ty, Weight::Normal, s.number_ink),
            s.ty.ch(4.0),
            Horizontal::Left,
        ));
    }
    row.height(Length::Fixed(s.ty.line)).into()
}

pub fn rates<'a, M: 'a>(s: Style, down: Option<f64>, up: Option<f64>) -> Element<'a, M> {
    Column::new().push(line(&s, true, down)).push(line(&s, false, up)).into()
}
