//! The details grid (SPEC §9.1): wind, humidity and UV over sunrise,
//! sunset and pressure.

use cosmic::iced::font::Weight;
use cosmic::iced::mouse;
use cosmic::iced::widget::canvas::{self, Frame, Geometry, Path};
use cosmic::iced::{Alignment, Length, Point, Radians, Rectangle, Vector};
use cosmic::widget::{self, Column, Row};
use cosmic::{Element, Renderer};

use common::graph::layered;
use common::ink::Ink;
use common::panel::Type;
use common::ui::{mono, space_m, space_s, space_xxs, space_xxxs};

use super::{Ctx, fade, opacity};
use crate::config::IconSet;
use crate::fl;
use crate::format;
use crate::icons;
use crate::model::Snapshot;

const MONO: Type = Type::new(14.0, 20.0);
const ICON: u16 = 14;

/// The System set's wind arrow, pointing where the wind blows to.
struct Arrow {
    deg: f32,
    dim: bool,
}

impl<M> canvas::Program<M, cosmic::Theme, Renderer> for Arrow {
    type State = ();

    fn draw(&self, _: &(), renderer: &Renderer, theme: &cosmic::Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let s = bounds.width / 12.0;
        let c = Vector::new(bounds.width / 2.0, bounds.height / 2.0);
        frame.translate(c);
        frame.rotate(Radians((self.deg + 180.0).to_radians()));
        let p = |x: f32, y: f32| Point::new((x - 6.0) * s, (y - 6.0) * s);
        let arrow = Path::new(|b| {
            b.move_to(p(6.0, 1.0));
            b.line_to(p(10.0, 10.0));
            b.line_to(p(6.0, 8.0));
            b.line_to(p(2.0, 10.0));
            b.close();
        });
        frame.fill(&arrow, fade(Ink::On.color(theme), self.dim));
        vec![frame.into_geometry()]
    }
}

/// One stat in a column taking `portion` of the row.
fn stat<'a, M: 'a>(key: String, icon: Option<Element<'a, M>>, value: String, dim: bool, portion: u16) -> Element<'a, M> {
    let value = Row::new()
        .spacing(space_xxxs() + 2)
        .align_y(Alignment::Center)
        .push_maybe(icon)
        .push(mono(value, MONO, Weight::Normal, Some(if dim { Ink::Muted } else { Ink::On })).wrapping(cosmic::iced::widget::text::Wrapping::None));
    Column::new().width(Length::FillPortion(portion)).push(widget::text::caption(key).class(Ink::Muted.text())).push(value).into()
}

pub fn details<'a, M: 'a>(s: &Snapshot, ctx: Ctx) -> Element<'a, M> {
    let c = &s.current;
    let detailed = ctx.icons == IconSet::Detailed;
    let wind_icon: Element<'a, M> = if detailed {
        // The glyph's needle points NE.
        widget::icon(icons::detailed("compass"))
            .size(ICON)
            .rotation(cosmic::iced::Rotation::Floating(Radians(((c.wind_deg + 135.0) as f32).to_radians())))
            .opacity(opacity(ctx.dim))
            .into()
    } else {
        layered(Arrow { deg: c.wind_deg as f32, dim: ctx.dim }, Length::Fixed(f32::from(ICON)), Length::Fixed(f32::from(ICON)))
    };
    let humidity_icon = detailed.then(|| Element::from(widget::icon(icons::detailed("humidity")).size(ICON).opacity(opacity(ctx.dim))));
    let wind = format!("{} {}", format::wind(c.wind_speed, ctx.units), format::compass(c.wind_deg));
    let row1 = Row::new()
        .spacing(space_s())
        .push(stat(fl!("wind"), Some(wind_icon), wind, ctx.dim, 6))
        .push(stat(fl!("humidity"), humidity_icon, format!("{}%", c.humidity.round() as i64), ctx.dim, 4))
        .push(stat(fl!("uv-index"), None, format::uv(c.uvi), ctx.dim, 5));
    let row2 = Row::new()
        .spacing(space_s())
        .push(stat(fl!("sunrise"), None, format::clock(c.sunrise, s.tz_offset, ctx.military), ctx.dim, 6))
        .push(stat(fl!("sunset"), None, format::clock(c.sunset, s.tz_offset, ctx.military), ctx.dim, 4))
        .push(stat(fl!("pressure"), None, format::pressure(c.pressure, ctx.units), ctx.dim, 5));
    widget::container(Column::new().spacing(space_s()).push(row1).push(row2)).padding([space_xxs(), space_m()]).width(Length::Fill).into()
}
