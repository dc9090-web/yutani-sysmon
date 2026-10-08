//! A quota meter: track, fill and pace tick, drawn on a canvas.

use cosmic::iced::widget::canvas::{self, Frame, Geometry, Path};
use cosmic::iced::{Color, Length, Point, Rectangle, Size, mouse};
use cosmic::{Element, Renderer};

use common::graph::layered;
use common::ink::Ink;

use crate::format::Level;

/// Opacity of stale fills (SPEC §7, DESIGN §5).
pub const STALE_ALPHA: f32 = 0.45;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Meter {
    /// 0–100, the displayed amount (used, or left).
    pub fill: f32,
    /// 0–100, where even spending would be; `None` hides the tick.
    pub tick: Option<f32>,
    pub level: Level,
    pub stale: bool,
    /// Track thickness; the canvas is as tall as the tick.
    pub track: f32,
    pub tick_height: f32,
    /// Draw a full bar at the limit whatever the amount (the panel's
    /// cue that doesn't rely on colour).
    pub solid_at_limit: bool,
}

impl Meter {
    fn ink(&self) -> Ink {
        match self.level {
            Level::Normal => Ink::Accent,
            Level::Warning => Ink::Warning,
            Level::Limit => Ink::Destructive,
        }
    }

    pub fn view<'a, M: 'a>(self, width: Length) -> Element<'a, M> {
        layered(self, width, Length::Fixed(self.tick_height))
    }
}

impl<M> canvas::Program<M, cosmic::Theme, Renderer> for Meter {
    type State = ();

    fn draw(&self, _: &(), renderer: &Renderer, theme: &cosmic::Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let (w, h) = (bounds.width, bounds.height);
        let t = self.track.min(h);
        let top = ((h - t) / 2.0).round();
        let radius = (t / 2.0).min(theme.cosmic().corner_radii.radius_xl[0]);
        let pill = |x: f32, width: f32| Path::rounded_rectangle(Point::new(x, top), Size::new(width, t), radius.into());
        let alpha = |c: Color| if self.stale { Color { a: c.a * STALE_ALPHA, ..c } } else { c };

        frame.fill(&pill(0.0, w), Ink::Component.color(theme));
        let frac = if self.solid_at_limit && self.level == Level::Limit { 1.0 } else { (self.fill / 100.0).clamp(0.0, 1.0) };
        if frac > 0.0 {
            frame.fill(&pill(0.0, (frac * w).max(t)), alpha(self.ink().color(theme)));
        }
        if let Some(tick) = self.tick.filter(|_| self.level != Level::Limit) {
            let x = ((tick / 100.0).clamp(0.0, 1.0) * w - 1.0).clamp(0.0, w - 2.0).round();
            frame.fill(&Path::rectangle(Point::new(x, 0.0), Size::new(2.0, h)), alpha(Ink::On.color(theme)));
        }
        vec![frame.into_geometry()]
    }
}
