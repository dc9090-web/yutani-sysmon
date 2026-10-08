//! The next 24 hours on a 112 px tall canvas, 296 wide in the design (DESIGN §4): the temperature
//! line, rain-chance bars, and labels every 3 hours.

use cosmic::iced::alignment::Vertical;
use cosmic::iced::mouse;
use cosmic::iced::widget::canvas::{self, Frame, Geometry, LineCap, LineJoin, Path, Stroke};
use cosmic::iced::widget::text::Alignment as TextAlign;
use cosmic::iced::{Font, Length, Pixels, Point, Rectangle, Size};
use cosmic::{Element, Renderer};

use common::graph::layered;
use common::ink::Ink;

use super::{fade, wx_now, wx_rain, wx_rain_fill, wx_temp};
use crate::format;
use crate::model::Hour;

pub const HEIGHT: f32 = 112.0;
/// The temperature band.
const TOP: f32 = 22.0;
const BAND: f32 = 44.0;
/// The rain bars end at the baseline.
const BASELINE: f32 = 96.0;
const POP_H: f32 = 18.0;
/// Inset of the first and last points.
const INSET: f32 = 6.0;

pub struct Hourly<'a> {
    pub hours: &'a [Hour],
    pub tz: i32,
    pub military: bool,
    pub dim: bool,
    /// Free tier: 3-hour steps, so every point is labelled.
    pub every_point: bool,
}

impl Hourly<'_> {
    /// The temperature range, at least 4° wide.
    fn range(&self) -> (f64, f64) {
        let (mn, mx) = self.hours.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), h| (a.min(h.temp), b.max(h.temp)));
        (mn, (mx - mn).max(4.0))
    }

    fn x(&self, i: usize, width: f32) -> f32 {
        let n = self.hours.len().max(2) - 1;
        INSET + i as f32 * (width - 2.0 * INSET) / n as f32
    }

    fn y(&self, t: f64) -> f32 {
        let (mn, span) = self.range();
        TOP + BAND - ((t - mn) / span) as f32 * BAND
    }

    /// The canvas's text summary: "Next 24 hours: 15° to 27°, rain chance up to 46% at 15:00".
    pub fn summary(&self) -> Option<String> {
        let (mn, _) = self.range();
        let mx = self.hours.iter().map(|h| h.temp).fold(f64::NEG_INFINITY, f64::max);
        let wettest = self.hours.iter().max_by(|a, b| a.pop.total_cmp(&b.pop))?;
        Some(crate::fl!(
            "a11y-hourly",
            min = format::temp(Some(mn)),
            max = format::temp(Some(mx)),
            pop = ((wettest.pop * 100.0).round() as i64),
            time = format::clock(Some(wettest.dt), self.tz, self.military)
        ))
    }
}

fn label(content: String, at: Point, size: f32, font: Font, color: cosmic::iced::Color, width: f32) -> canvas::Text {
    // Keep labels near the edges inside the canvas.
    let half = content.chars().count() as f32 * size * 0.3;
    let x = at.x.clamp(half, width - half);
    canvas::Text {
        content,
        position: Point::new(x, at.y),
        color,
        size: Pixels(size),
        font,
        align_x: TextAlign::Center,
        align_y: Vertical::Bottom,
        ..canvas::Text::default()
    }
}

impl<M> canvas::Program<M, cosmic::Theme, Renderer> for Hourly<'_> {
    type State = ();

    fn draw(&self, _: &(), renderer: &Renderer, theme: &cosmic::Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        if self.hours.is_empty() {
            return vec![frame.into_geometry()];
        }
        let dim = self.dim;
        let w = bounds.width;
        let mono = cosmic::font::mono();
        let sans = cosmic::font::default();

        frame.fill_rectangle(Point::new(0.0, BASELINE), Size::new(w, 1.0), fade(Ink::ComponentDivider.color(theme), dim));

        for (i, h) in self.hours.iter().enumerate() {
            let height = h.pop.clamp(0.0, 1.0) as f32 * POP_H;
            if height > 0.5 {
                let bar = Path::rounded_rectangle(Point::new(self.x(i, w) - 4.5, BASELINE - height), Size::new(9.0, height), 1.5.into());
                frame.fill(&bar, fade(wx_rain_fill(theme), dim));
            }
        }

        let line = Path::new(|b| {
            for (i, h) in self.hours.iter().enumerate() {
                let p = Point::new(self.x(i, w), self.y(h.temp));
                if i == 0 { b.move_to(p) } else { b.line_to(p) }
            }
        });
        frame.stroke(
            &line,
            Stroke::default().with_color(fade(wx_temp(theme), dim)).with_width(2.0).with_line_join(LineJoin::Round).with_line_cap(LineCap::Round),
        );
        frame.fill(&Path::circle(Point::new(self.x(0, w), self.y(self.hours[0].temp)), 3.5), fade(wx_now(theme), dim));

        let step = if self.every_point { 1 } else { 3 };
        let on = fade(Ink::On.color(theme), dim);
        let muted = fade(Ink::Muted.color(theme), dim);
        for (i, h) in self.hours.iter().enumerate().step_by(step) {
            let x = self.x(i, w);
            frame.fill_text(label(format::temp(Some(h.temp)), Point::new(x, self.y(h.temp) - 6.0), 12.0, mono, on, w));
            let hour = if i == 0 { crate::fl!("now") } else { format::hour(h.dt, self.tz, self.military) };
            frame.fill_text(label(hour, Point::new(x, HEIGHT - 1.0), 11.0, sans, muted, w));
            if let Some(p) = format::pop(h.pop) {
                frame.fill_text(label(p, Point::new(x, BASELINE - POP_H - 1.0), 10.0, mono, fade(wx_rain(theme), dim), w));
            }
        }
        vec![frame.into_geometry()]
    }
}

pub fn view<'a, M: 'a>(h: Hourly<'a>) -> Element<'a, M> {
    layered(h, Length::Fill, Length::Fixed(HEIGHT))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Condition;

    /// The design width (DESIGN §4); the canvas fills what the popup gives it.
    const WIDTH: f32 = 296.0;

    fn hours(temps: &[f64]) -> Vec<Hour> {
        temps
            .iter()
            .enumerate()
            .map(|(i, t)| Hour { dt: 1_791_417_600 + i as i64 * 3600, temp: *t, pop: i as f64 / 50.0, condition: Condition::default() })
            .collect()
    }

    #[test]
    fn geometry() {
        let hs = hours(&[15.0; 24].iter().enumerate().map(|(i, _)| 15.0 + i as f64 / 2.0).collect::<Vec<_>>());
        let h = Hourly { hours: &hs, tz: 39600, military: true, dim: false, every_point: false };
        assert_eq!(h.x(0, WIDTH), 6.0);
        assert_eq!(h.x(23, WIDTH), 290.0);
        // The warmest point is at the top of the band, the coolest at the bottom.
        assert_eq!(h.y(15.0), TOP + BAND);
        assert_eq!(h.y(26.5), TOP);
    }

    #[test]
    fn flat_day_uses_a_4_degree_span() {
        let hs = hours(&[20.0; 9]);
        let h = Hourly { hours: &hs, tz: 0, military: true, dim: false, every_point: true };
        assert_eq!(h.range(), (20.0, 4.0));
        assert_eq!(h.y(20.0), TOP + BAND);
    }

    #[test]
    fn summary_text() {
        let hs = hours(&[15.0, 27.0, 20.0]);
        let h = Hourly { hours: &hs, tz: 39600, military: true, dim: false, every_point: false };
        // The wettest point is the last one (pop 0.04): 13:00 Sydney.
        assert_eq!(h.summary().unwrap(), "Next 24 hours: 15° to 27°, rain chance up to 4% at 13:00");
    }
}
