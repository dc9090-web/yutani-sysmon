//! The panel button's content: the glyph, then the `TS` chunk, a label over
//! a value in a fixed-width cell so the width never moves.

use cosmic::Element;
use cosmic::iced::alignment::Horizontal;
use cosmic::iced::font::Weight;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{Column, Row};

use common::ink::Ink;
use common::panel::{Panel, SizeClass, Type};
use common::ui::{cell, mono, space_xs, space_xxxs};

use crate::model::Tone;

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
        Tone::Err => Ink::Destructive,
    }
}

/// Label and value type: stepped up on M and larger panels, like the siblings.
fn types(p: &Panel) -> (Type, Type) {
    if p.class >= SizeClass::M { (Type::new(11.0, 14.0), Type::new(14.0, 20.0)) } else { (Type::new(10.0, 12.0), Type::new(12.0, 17.0)) }
}

fn chunk<'a, M: 'a>(c: Chunk, p: &Panel) -> Element<'a, M> {
    let (label, value) = types(p);
    let w = value.ch(c.chars);
    let weight = if c.tone == Tone::Busy { Weight::Normal } else { Weight::Semibold };
    Column::new()
        .align_x(Alignment::Center)
        .push(cell(mono(c.label, label, Weight::Semibold, Some(Ink::Muted)), w, Horizontal::Center))
        .push(cell(mono(c.value, value, weight, Some(tone_ink(c.tone))), w, Horizontal::Center))
        .into()
}

pub fn content<'a, M: 'a>(icon: Element<'a, M>, chunk_: Option<Chunk>, p: &Panel) -> Element<'a, M> {
    if p.horizontal {
        Row::new().spacing(space_xs()).align_y(Alignment::Center).push(icon).push_maybe(chunk_.map(|c| chunk(c, p))).into()
    } else {
        Column::new().spacing(space_xxxs()).align_x(Alignment::Center).width(Length::Shrink).push(icon).push_maybe(chunk_.map(|c| chunk(c, p))).into()
    }
}
