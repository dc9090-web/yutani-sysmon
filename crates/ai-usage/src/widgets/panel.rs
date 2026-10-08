//! The panel button's chunks: a label over a value, a bar, or both.

use cosmic::Element;
use cosmic::iced::alignment::Horizontal;
use cosmic::iced::font::Weight;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{Column, Row};

use common::ink::Ink;
use common::panel::{Panel, Type};
use common::ui::{mono, space_xxs};

use crate::config::PanelStyle;
use crate::format::{Level, pct_cell};
use crate::widgets::meter::Meter;

/// What a chunk shows under its label.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Window {
        /// 0–100, used or left.
        shown: f32,
        /// Pace tick position, 0–100, already flipped in Left mode.
        tick: Option<f32>,
        level: Level,
    },
    /// The session countdown, compact form.
    Reset(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    pub label: String,
    pub value: Value,
}

/// Sizes for one panel.
#[derive(Debug, Clone, Copy)]
pub struct Look {
    pub horizontal: bool,
    pub style: PanelStyle,
    pub label: Type,
    pub value: Type,
    pub bar: f32,
    pub stale: bool,
}

impl Look {
    pub fn of(p: &Panel, style: PanelStyle, stale: bool, thickness: f32) -> Self {
        let style = if p.vertical_xs() { PanelStyle::Bars } else { style };
        let (label, value, bar) =
            if p.horizontal { (Type::new(10.0, 12.0), Type::new(12.0, 17.0), 32.0) } else { (Type::new(9.0, 11.0), Type::new(11.0, 14.0), 24.0) };
        // Two lines must fit the panel's thickness.
        let value = if p.horizontal { Type::new(value.size, value.line.min((thickness - label.line).floor())) } else { value };
        Self { horizontal: p.horizontal, style, label, value, bar, stale }
    }

    /// Fixed chunk width: the widest of the label and the second line, so
    /// values never change it.
    fn width(&self, value: &Value) -> f32 {
        let label = self.label.ch(5.0);
        let second = match (value, self.style) {
            (Value::Reset(_), _) => self.value.ch(5.0),
            (_, PanelStyle::Percent) => self.value.ch(4.0),
            (_, PanelStyle::Bars) => self.bar,
            (_, PanelStyle::Both) => self.value.ch(4.0) + f32::from(space_xxs()) + self.bar,
        };
        label.max(second)
    }
}

pub fn chunk<'a, M: 'a>(c: &Chunk, look: &Look) -> Element<'a, M> {
    let align = if look.horizontal { Alignment::Start } else { Alignment::Center };
    let label = mono(c.label.clone(), look.label, Weight::Semibold, Some(Ink::Muted));
    let second: Element<'a, M> = match &c.value {
        Value::Reset(s) => value_text(format!("{s:>5}"), 5.0, None, look),
        Value::Window { shown, tick, level } => {
            let ink = match (look.stale, level) {
                (true, _) => Some(Ink::Muted),
                (false, Level::Normal) => None,
                (false, Level::Warning) => Some(Ink::Warning),
                (false, Level::Limit) => Some(Ink::Destructive),
            };
            let value = || value_text(pct_cell(*shown, *level == Level::Limit), 4.0, ink, look);
            let bar = || {
                Meter { fill: *shown, tick: *tick, level: *level, stale: look.stale, track: 6.0, tick_height: 10.0, solid_at_limit: true }
                    .view(Length::Fixed(look.bar))
            };
            match look.style {
                PanelStyle::Percent => value(),
                PanelStyle::Bars => bar(),
                PanelStyle::Both => Row::new().spacing(space_xxs()).align_y(Alignment::Center).push(value()).push(bar()).into(),
            }
        }
    };
    Column::new().width(Length::Fixed(look.width(&c.value))).align_x(align).push(label).push(second).into()
}

/// Mono text right-aligned in an `n`-character cell.
fn value_text<'a, M: 'a>(s: String, n: f32, ink: Option<Ink>, look: &Look) -> Element<'a, M> {
    common::ui::cell(mono(s, look.value, Weight::Normal, ink), look.value.ch(n), Horizontal::Right)
}

/// The robot, then the chunks.
pub fn content<'a, M: 'a>(robot: Element<'a, M>, chunks: &[Chunk], look: &Look) -> Element<'a, M> {
    if look.horizontal {
        let mut row = Row::new().spacing(common::ui::space_xs()).align_y(Alignment::Center).push(robot);
        for c in chunks {
            row = row.push(chunk(c, look));
        }
        row.into()
    } else {
        let mut col = Column::new().spacing(space_xxs()).align_x(Alignment::Center).push(robot);
        for c in chunks {
            col = col.push(chunk(c, look));
        }
        col.into()
    }
}
