//! The Cyborg avatars and their session ring (SPEC §7.1, DESIGN §2b).

use std::cell::Cell;
use std::f32::consts::{FRAC_PI_2, TAU};
use std::sync::LazyLock;

use cosmic::Element;
use cosmic::iced::widget::canvas::{self, Cache, Geometry, LineCap, Path, Stroke, path::Arc};
use cosmic::iced::{Color, Length, Point, Rectangle, mouse};
use cosmic::widget::{self, image::Handle};

use common::graph::layered;
use common::ink::Ink;

use crate::format::Level;
use crate::widgets::meter::STALE_ALPHA;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Avatar {
    Cyan,
    Red,
}

/// The pre-scaled PNG sizes.
const SIZES: [u16; 10] = [24, 32, 40, 48, 56, 64, 80, 96, 112, 128];

macro_rules! pngs {
    ($colour:literal) => {
        [
            include_bytes!(concat!("../../resources/icons/avatars/ai-usage-cyborg-", $colour, "-24.png")).as_slice(),
            include_bytes!(concat!("../../resources/icons/avatars/ai-usage-cyborg-", $colour, "-32.png")).as_slice(),
            include_bytes!(concat!("../../resources/icons/avatars/ai-usage-cyborg-", $colour, "-40.png")).as_slice(),
            include_bytes!(concat!("../../resources/icons/avatars/ai-usage-cyborg-", $colour, "-48.png")).as_slice(),
            include_bytes!(concat!("../../resources/icons/avatars/ai-usage-cyborg-", $colour, "-56.png")).as_slice(),
            include_bytes!(concat!("../../resources/icons/avatars/ai-usage-cyborg-", $colour, "-64.png")).as_slice(),
            include_bytes!(concat!("../../resources/icons/avatars/ai-usage-cyborg-", $colour, "-80.png")).as_slice(),
            include_bytes!(concat!("../../resources/icons/avatars/ai-usage-cyborg-", $colour, "-96.png")).as_slice(),
            include_bytes!(concat!("../../resources/icons/avatars/ai-usage-cyborg-", $colour, "-112.png")).as_slice(),
            include_bytes!(concat!("../../resources/icons/avatars/ai-usage-cyborg-", $colour, "-128.png")).as_slice(),
        ]
    };
}

/// One handle per file, made once so the renderer uploads each image once.
static HANDLES: LazyLock<[[Handle; 10]; 2]> = LazyLock::new(|| [pngs!("cyan").map(Handle::from_bytes), pngs!("red").map(Handle::from_bytes)]);

/// The smallest PNG at least `px` physical pixels across, so it's only ever
/// scaled down, never up.
fn handle(avatar: Avatar, px: f32) -> Handle {
    let i = SIZES.iter().position(|s| f32::from(*s) >= px).unwrap_or(SIZES.len() - 1);
    HANDLES[avatar as usize][i].clone()
}

/// Ring stroke, gap, and image diameter for an icon box of `size` px.
pub fn geometry(size: f32) -> (f32, f32, f32) {
    let stroke = (size * 0.07).round().max(2.0);
    let gap = if size >= 48.0 { 2.0 } else { 1.0 };
    (stroke, gap, size - 2.0 * (stroke + gap))
}

/// What the ring shows. `used` is the session % used; `None` draws the track only.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ring {
    pub used: Option<f32>,
    pub stale: bool,
}

impl Ring {
    /// 0–1 of the circle, and the arc's ink. A full ring at the limit.
    fn arc(&self) -> Option<(f32, Ink, bool)> {
        let used = self.used?;
        Some(match Level::of(used) {
            Level::Limit => (1.0, Ink::Destructive, true),
            Level::Warning => ((100.0 - used) / 100.0, Ink::Warning, false),
            Level::Normal => ((100.0 - used) / 100.0, Ink::Accent, false),
        })
    }

    /// "Session 58% left", "Session limit reached", "Session: no data".
    pub fn label(&self) -> String {
        match self.used {
            None => crate::fl!("ring-no-data"),
            Some(u) if Level::of(u) == Level::Limit => crate::fl!("ring-limit"),
            Some(u) => crate::fl!("ring-left", pct = ((100.0 - u).round() as i64)),
        }
    }
}

struct Program {
    ring: Ring,
    stroke: f32,
}

/// What a drawing depends on: session used (hundredths), stale, track and arc colours.
type Key = (Option<u32>, bool, [u8; 4], [u8; 4]);

/// The cache and what it was drawn for. It's redrawn only when that changes.
#[derive(Default)]
struct State {
    cache: Cache<cosmic::Renderer>,
    drawn: Cell<Option<Key>>,
}

impl<M> canvas::Program<M, cosmic::Theme, cosmic::Renderer> for Program {
    type State = State;

    fn draw(&self, state: &State, renderer: &cosmic::Renderer, theme: &cosmic::Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let arc = self.ring.arc();
        let track = Ink::Component.color(theme);
        let ink = arc.map_or(track, |(_, i, _)| i.color(theme));
        let key = (self.ring.used.map(|u| (u * 100.0).round() as u32), self.ring.stale, track.into_rgba8(), ink.into_rgba8());
        if state.drawn.get() != Some(key) {
            state.cache.clear();
            state.drawn.set(Some(key));
        }
        let geometry = state.cache.draw(renderer, bounds.size(), |frame| {
            let size = bounds.width.min(bounds.height);
            let center = Point::new(bounds.width / 2.0, bounds.height / 2.0);
            let radius = (size - self.stroke) / 2.0;
            frame.stroke(&Path::circle(center, radius), Stroke::default().with_color(track).with_width(self.stroke));
            if let Some((frac, _, full)) = arc
                && frac > 0.0
            {
                let color = if self.ring.stale { Color { a: ink.a * STALE_ALPHA, ..ink } } else { ink };
                let path = if full {
                    Path::circle(center, radius)
                } else {
                    let start = -FRAC_PI_2;
                    Path::new(|b| b.arc(Arc { center, radius, start_angle: start.into(), end_angle: (start + frac * TAU).into() }))
                };
                let cap = if full { LineCap::Butt } else { LineCap::Round };
                frame.stroke(&path, Stroke::default().with_color(color).with_width(self.stroke).with_line_cap(cap));
            }
        });
        vec![geometry]
    }
}

/// The avatar in a `size` px box with its ring. `scale` is the output's
/// scale factor, for picking the PNG.
pub fn view<'a, M: 'a>(avatar: Avatar, ring: Ring, size: f32, scale: f32) -> Element<'a, M> {
    let (stroke, _, img) = geometry(size);
    let image = widget::image(handle(avatar, img * scale)).width(Length::Fixed(img)).height(Length::Fixed(img));
    cosmic::iced::widget::stack([
        widget::container(image).center(Length::Fixed(size)).into(),
        layered(Program { ring, stroke }, Length::Fixed(size), Length::Fixed(size)),
    ])
    .width(Length::Fixed(size))
    .height(Length::Fixed(size))
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_geometry() {
        // DESIGN §2b: box, stroke, gap, image.
        for (size, stroke, gap, img) in [(32.0, 2.0, 1.0, 26.0), (40.0, 3.0, 1.0, 32.0), (48.0, 3.0, 2.0, 38.0), (56.0, 4.0, 2.0, 44.0)] {
            assert_eq!(geometry(size), (stroke, gap, img), "{size}");
        }
    }

    #[test]
    fn arc_is_session_left() {
        let r = |used| Ring { used: Some(used), stale: false }.arc().unwrap();
        assert_eq!(r(42.0), (0.58, Ink::Accent, false));
        let (f, ink, full) = r(87.0);
        assert!((f - 0.13).abs() < 1e-6 && ink == Ink::Warning && !full);
        assert_eq!(r(100.0), (1.0, Ink::Destructive, true));
        assert_eq!(Ring { used: None, stale: false }.arc(), None);
    }

    #[test]
    fn labels() {
        assert_eq!(Ring { used: Some(42.0), stale: false }.label(), "Session 58% left");
        assert_eq!(Ring { used: Some(100.0), stale: true }.label(), "Session limit reached");
        assert_eq!(Ring { used: None, stale: false }.label(), "Session: no data");
    }

    #[test]
    fn png_choice() {
        let i = |px: f32| SIZES.iter().position(|s| f32::from(*s) >= px).unwrap_or(SIZES.len() - 1);
        assert_eq!(SIZES[i(26.0)], 32);
        assert_eq!(SIZES[i(52.0)], 56);
        assert_eq!(SIZES[i(88.0)], 96);
        assert_eq!(SIZES[i(300.0)], 128);
    }
}
