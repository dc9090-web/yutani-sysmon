//! The panel button's content (DESIGN §3): the reticle, then `P2P` / `WEB`
//! (and `PORT`) chunks, each a label over a value in fixed-width cells.

use cosmic::Element;
use cosmic::iced::alignment::Horizontal;
use cosmic::iced::font::Weight;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{Column, Row};

use common::ink::Ink;
use common::panel::{Panel, Type};
use common::ui::{cell, mono, space_xs, space_xxxs};

use crate::model::Tone;

const LABEL: Type = Type::new(10.0, 12.0);
const VALUE: Type = Type::new(12.0, 17.0);

pub struct Chunk {
    pub label: String,
    pub value: String,
    pub tone: Tone,
    /// Width in characters of the value cell.
    pub chars: f32,
}

pub fn tone_ink(t: Tone) -> Ink {
    match t {
        Tone::Off => Ink::Muted,
        Tone::Busy => Ink::On,
        Tone::On => Ink::Success,
        Tone::Warn => Ink::Warning,
        Tone::Err => Ink::Destructive,
    }
}

fn chunk<'a, M: 'a>(c: Chunk) -> Element<'a, M> {
    let w = VALUE.ch(c.chars);
    let weight = if c.tone == Tone::Busy { Weight::Normal } else { Weight::Semibold };
    Column::new()
        .align_x(Alignment::Center)
        .push(cell(mono(c.label, LABEL, Weight::Semibold, Some(Ink::Muted)), w, Horizontal::Center))
        .push(cell(mono(c.value, VALUE, weight, Some(tone_ink(c.tone))), w, Horizontal::Center))
        .into()
}

pub fn content<'a, M: 'a>(icon: Element<'a, M>, chunks: Vec<Chunk>, p: &Panel) -> Element<'a, M> {
    if p.horizontal {
        let mut row = Row::new().spacing(space_xs()).align_y(Alignment::Center).push(icon);
        for c in chunks {
            row = row.push(chunk(c));
        }
        row.into()
    } else {
        let mut col = Column::new().spacing(space_xxxs()).align_x(Alignment::Center).width(Length::Shrink).push(icon);
        for c in chunks {
            col = col.push(chunk(c));
        }
        col.into()
    }
}
