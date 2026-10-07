//! Colours, always read from the live theme (`theme.cosmic()`), never
//! hard-coded. The series colours are fixed palette entries, never the
//! user's accent.

use cosmic::iced::Color;
use cosmic::theme::{Svg, Text};
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ink {
    /// `palette.accent_blue`: net download, disk read.
    Blue,
    /// `palette.accent_orange`: net upload, disk write.
    Orange,
    /// `palette.accent_indigo`: CPU.
    Indigo,
    /// `palette.accent_purple`: GPU.
    Purple,
    /// `palette.accent_green`: memory.
    Green,
    /// `on-bg-muted`: neutral 7 (dark) / 6 (light).
    Muted,
    Warning,
    Destructive,
    Success,
    /// `background.on`.
    On,
    /// `accent.on`, for icons on an accent fill.
    OnAccent,
    /// `background.component.divider`: inset dividers, graph grid.
    ComponentDivider,
    /// `background.component.base`: graph well, meter track.
    Component,
}

impl Ink {
    pub fn color(self, theme: &cosmic::Theme) -> Color {
        let c = theme.cosmic();
        let bg = c.background(theme.transparent);
        let srgba = match self {
            Ink::Blue => c.palette.accent_blue,
            Ink::Orange => c.palette.accent_orange,
            Ink::Indigo => c.palette.accent_indigo,
            Ink::Purple => c.palette.accent_purple,
            Ink::Green => c.palette.accent_green,
            Ink::Muted => {
                if c.is_dark {
                    c.palette.neutral_7
                } else {
                    c.palette.neutral_6
                }
            }
            Ink::Warning => c.warning.base,
            Ink::Destructive => c.destructive.base,
            Ink::Success => c.success.base,
            Ink::On => bg.on,
            Ink::OnAccent => c.accent.on,
            Ink::ComponentDivider => bg.component.divider,
            Ink::Component => bg.component.base,
        };
        srgba.into()
    }

    /// Text class that follows theme changes.
    pub fn text(self) -> Text {
        // `Text::Custom` takes a plain `fn`, so one per ink.
        macro_rules! f {
            ($ink:expr) => {
                Text::Custom(|t| cosmic::iced::widget::text::Style { color: Some($ink.color(t)), ..Default::default() })
            };
        }
        match self {
            Ink::Blue => f!(Ink::Blue),
            Ink::Orange => f!(Ink::Orange),
            Ink::Indigo => f!(Ink::Indigo),
            Ink::Purple => f!(Ink::Purple),
            Ink::Green => f!(Ink::Green),
            Ink::Muted => f!(Ink::Muted),
            Ink::Warning => f!(Ink::Warning),
            Ink::Destructive => f!(Ink::Destructive),
            Ink::Success => f!(Ink::Success),
            Ink::On => f!(Ink::On),
            Ink::OnAccent => f!(Ink::OnAccent),
            Ink::ComponentDivider => f!(Ink::ComponentDivider),
            Ink::Component => f!(Ink::Component),
        }
    }

    /// Symbolic-icon class in this ink.
    pub fn svg(self) -> Svg {
        Svg::Custom(Rc::new(move |t| cosmic::iced::widget::svg::Style { color: Some(self.color(t)) }))
    }
}

/// Graph area fill: the series colour at 25 % (dark) or 20 % (light).
pub fn fill(theme: &cosmic::Theme, ink: Ink) -> Color {
    let a = if theme.cosmic().is_dark { 0.25 } else { 0.20 };
    Color { a, ..ink.color(theme) }
}
