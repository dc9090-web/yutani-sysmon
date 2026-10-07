//! Panel orientation and size, from the applet context.

use cosmic::applet::cosmic_panel_config::PanelSize;
use cosmic::applet::{Context, Size};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SizeClass {
    XS,
    S,
    M,
    L,
    XL,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Panel {
    pub horizontal: bool,
    pub class: SizeClass,
    /// `suggested_size(true)`: the symbolic icon size.
    pub icon: (f32, f32),
}

impl Panel {
    pub fn of(ctx: &Context) -> Self {
        let (w, h) = ctx.suggested_size(true);
        let class = match &ctx.size {
            Size::PanelSize(PanelSize::XS) => SizeClass::XS,
            Size::PanelSize(PanelSize::S) => SizeClass::S,
            Size::PanelSize(PanelSize::M) => SizeClass::M,
            Size::PanelSize(PanelSize::L) => SizeClass::L,
            Size::PanelSize(PanelSize::XL) => SizeClass::XL,
            // Custom sizes: classify by the icon size they produce.
            _ => match h {
                0..=17 => SizeClass::XS,
                18..=23 => SizeClass::S,
                24..=29 => SizeClass::M,
                30..=39 => SizeClass::L,
                _ => SizeClass::XL,
            },
        };
        Self { horizontal: ctx.is_horizontal(), class, icon: (f32::from(w), f32::from(h)) }
    }

    pub fn vertical_xs(&self) -> bool {
        !self.horizontal && self.class == SizeClass::XS
    }
}

/// Text size and line height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Type {
    pub size: f32,
    pub line: f32,
}

impl Type {
    pub const fn new(size: f32, line: f32) -> Self {
        Self { size, line }
    }

    /// Width of `n` characters of the mono font (Noto Sans Mono advances
    /// are 0.6 em).
    pub fn ch(&self, n: f32) -> f32 {
        (n * 0.6 * self.size).ceil()
    }
}
