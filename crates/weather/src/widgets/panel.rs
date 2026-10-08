//! The panel button's content (SPEC §8, DESIGN §3): the icon (with the
//! alert dot), the temperature in a fixed 4-character field, and optionally
//! the city and today's high / low.

use cosmic::iced::alignment::{Horizontal, Vertical};
use cosmic::iced::font::Weight;
use cosmic::iced::widget::text::LineHeight;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{self, Column, Row, icon};
use cosmic::{Element, theme};

use common::ink::Ink;
use common::panel::{Panel, SizeClass, Type};
use common::ui::{cell, mono, space_xxs, space_xxxs};

use crate::format;

/// The panel temperature, a step heavier than regular.
const TEMP_WEIGHT: Weight = Weight::Medium;
const DOT: f32 = 7.0;
const HALO: f32 = 2.0;

/// What the panel shows.
pub struct Content {
    pub icon: icon::Handle,
    /// `None`: no key, no location or no data yet; the icon alone.
    pub temp: Option<f64>,
    pub city: Option<String>,
    /// Today's high and low.
    pub hilo: Option<(f64, f64)>,
    pub alert: bool,
    pub stale: bool,
}

/// The temperature's type at each panel size.
fn temp_type(class: SizeClass) -> Type {
    match class {
        SizeClass::XS | SizeClass::S => Type::new(14.0, 20.0),
        SizeClass::M | SizeClass::L => Type::new(16.0, 22.0),
        SizeClass::XL => Type::new(20.0, 28.0),
    }
}

/// The icon in a `size` box, with the warning dot at its top-right.
fn icon_with_dot<'a, M: 'a>(handle: icon::Handle, size: f32, alert: bool, stale: bool) -> Element<'a, M> {
    let ink = if stale { Ink::Muted } else { Ink::On };
    // Room for the halo is kept with or without an alert, so the width never changes;
    // the same room below keeps the icon centred on the temperature.
    let icon = widget::container(widget::icon(handle).size(size as u16).class(ink.svg())).padding([HALO, HALO, HALO, 0.0]);
    if !alert {
        return icon.into();
    }
    let dot = widget::container(widget::space().width(Length::Fixed(DOT + 2.0 * HALO)).height(Length::Fixed(DOT + 2.0 * HALO))).class(
        theme::Container::custom(|t| widget::container::Style {
            background: Some(Ink::Warning.color(t).into()),
            border: cosmic::iced::Border { radius: (DOT / 2.0 + HALO).into(), width: HALO, color: t.cosmic().background(t.transparent).base.into() },
            ..Default::default()
        }),
    );
    // The halo sits partly outside the icon's corner.
    let corner = widget::container(dot).width(Length::Fixed(size + HALO)).height(Length::Fixed(size + HALO)).align_x(Horizontal::Right).align_y(Vertical::Top);
    cosmic::iced::widget::stack([icon.into(), corner.into()]).into()
}

pub fn content<'a, M: 'a>(c: Content, p: &Panel) -> Element<'a, M> {
    let size = if p.horizontal { p.icon.1 } else { p.icon.0 };
    let icon = icon_with_dot(c.icon, size, c.alert, c.stale);
    let Some(t) = c.temp else { return icon };
    let ink = if c.stale { Ink::Muted } else { Ink::On };

    if !p.horizontal {
        let ty = if p.class <= SizeClass::S { Type::new(12.0, 17.0) } else { Type::new(14.0, 20.0) };
        return Column::new().spacing(space_xxxs()).align_x(Alignment::Center).push(icon).push(mono(format::temp(Some(t)), ty, TEMP_WEIGHT, Some(ink))).into();
    }

    let ty = temp_type(p.class);
    let temp: Element<'a, M> = match c.city {
        Some(city) => {
            let small = Type::new(12.0, 17.0);
            let name = widget::text(format::ellipsize(&city, 10))
                .size(10.0)
                .line_height(LineHeight::Absolute(12.0.into()))
                .font(cosmic::font::semibold())
                .wrapping(cosmic::iced::widget::text::Wrapping::None)
                .class(Ink::Muted.text());
            // The temperature is centred under the city, in a fixed 4-character cell.
            Column::new()
                .align_x(Alignment::Center)
                .push(name)
                .push(cell(mono(format::temp(Some(t)), small, TEMP_WEIGHT, Some(ink)), small.ch(4.0), Horizontal::Center))
                .into()
        }
        // Next to the icon; the 4-character cell keeps the width fixed.
        None => cell(mono(format::temp(Some(t)), ty, TEMP_WEIGHT, Some(ink)), ty.ch(4.0), Horizontal::Left),
    };
    let hilo = c.hilo.map(|(hi, lo)| {
        let small = Type::new(10.0, 12.0);
        Column::new().push(mono(format::panel_temp(Some(hi)), small, Weight::Normal, Some(Ink::Muted))).push(mono(
            format::panel_temp(Some(lo)),
            small,
            Weight::Normal,
            Some(Ink::Muted),
        ))
    });
    Row::new().spacing(space_xxs()).align_y(Alignment::Center).push(icon).push(temp).push_maybe(hilo).into()
}
