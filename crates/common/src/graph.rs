//! Line graphs on a `canvas`: the network/disk graph (area series plus a
//! line series on a shared scale) and the single-series percentage graphs.

use cosmic::iced::mouse;
use cosmic::iced::widget::canvas::{self, Frame, Geometry, LineCap, LineJoin, Path, Stroke};
use cosmic::iced::{Length, Point, Rectangle};
use cosmic::{Element, Renderer};

use crate::format::nice_max;
use crate::ink::{self, Ink};
use crate::ring::{HISTORY, Ring};

/// Shared Y maximum for two series: `nice(max × 1.05)`, 1000 when all zero.
pub fn shared_max(a: &Ring, b: &Ring) -> f32 {
    let m = a.max().max(b.max()) as f64;
    if m <= 0.0 { 1000.0 } else { nice_max(m * 1.05) as f32 }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grid {
    None,
    /// One line at 50 %.
    Half,
    /// Three lines at 25/50/75 %.
    Quarters,
}

/// One graph. Borrowed from the app state for one frame.
pub struct Graph<'a> {
    /// Drawn as area fill plus stroke.
    pub area: Option<(&'a Ring, Ink)>,
    /// Drawn as a line only, above `area`.
    pub line: Option<(&'a Ring, Ink)>,
    pub max: f32,
    pub grid: Grid,
    /// Inset from every edge.
    pub pad: f32,
    pub stroke: f32,
}

impl Graph<'_> {
    /// Points of one run of known samples, newest at the right edge. A
    /// `None` sample ends a run, so it draws as a gap.
    fn runs(&self, ring: &Ring, size: cosmic::iced::Size) -> Vec<Vec<Point>> {
        let (w, h, pad) = (size.width, size.height, self.pad);
        let step = (w - 2.0 * pad) / (HISTORY - 1) as f32;
        let off = HISTORY - ring.len();
        let mut runs: Vec<Vec<Point>> = Vec::new();
        let mut cur: Vec<Point> = Vec::new();
        for (i, v) in ring.iter().enumerate() {
            match v {
                Some(v) => {
                    let frac = (v / self.max).clamp(0.0, 1.0);
                    cur.push(Point::new(pad + (i + off) as f32 * step, h - pad - frac * (h - 2.0 * pad)));
                }
                None if !cur.is_empty() => runs.push(std::mem::take(&mut cur)),
                None => {}
            }
        }
        if !cur.is_empty() {
            runs.push(cur);
        }
        runs
    }
}

fn polyline(pts: &[Point]) -> Path {
    Path::new(|b| {
        b.move_to(pts[0]);
        for p in &pts[1..] {
            b.line_to(*p);
        }
    })
}

impl<M> canvas::Program<M, cosmic::Theme, Renderer> for Graph<'_> {
    type State = ();

    fn draw(&self, _: &(), renderer: &Renderer, theme: &cosmic::Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let size = bounds.size();
        let lines = match self.grid {
            Grid::None => &[][..],
            Grid::Half => &[0.5][..],
            Grid::Quarters => &[0.25, 0.5, 0.75][..],
        };
        let grid_ink = Ink::ComponentDivider.color(theme);
        for f in lines {
            let y = (size.height * f).round() + 0.5;
            frame.stroke(&Path::line(Point::new(0.0, y), Point::new(size.width, y)), Stroke::default().with_color(grid_ink).with_width(1.0));
        }
        let stroke =
            |ink: Ink| Stroke::default().with_color(ink.color(theme)).with_width(self.stroke).with_line_join(LineJoin::Round).with_line_cap(LineCap::Round);
        let base = size.height - self.pad;
        if let Some((ring, ink)) = self.area {
            for run in self.runs(ring, size) {
                let (first, last) = (run[0], run[run.len() - 1]);
                let area = Path::new(|b| {
                    b.move_to(Point::new(first.x, base));
                    for p in &run {
                        b.line_to(*p);
                    }
                    b.line_to(Point::new(last.x, base));
                    b.close();
                });
                frame.fill(&area, ink::fill(theme, ink));
                if run.len() > 1 {
                    frame.stroke(&polyline(&run), stroke(ink));
                }
            }
        }
        if let Some((ring, ink)) = self.line {
            for run in self.runs(ring, size) {
                if run.len() > 1 {
                    frame.stroke(&polyline(&run), stroke(ink));
                }
            }
        }
        vec![frame.into_geometry()]
    }
}

/// A canvas in a renderer layer of its own. On the panel's popup surface
/// only the last canvas drawn into a layer shows up (seen in Yutani,
/// 2026-10-07); a `stack` draws each child after its first in a new layer,
/// so each canvas sits on an empty space of the same size.
pub fn layered<'a, M: 'a, P>(program: P, w: Length, h: Length) -> Element<'a, M>
where
    P: canvas::Program<M, cosmic::Theme, Renderer> + 'a,
{
    cosmic::iced::widget::stack([cosmic::widget::space().width(w).height(h).into(), cosmic::widget::canvas(program).width(w).height(h).into()])
        .width(w)
        .height(h)
        .into()
}
