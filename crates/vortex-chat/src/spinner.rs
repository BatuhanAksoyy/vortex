//! The "✻ Pondering…" row shown while the agent works: a cycling glyph in
//! the timeline's dot column and a verb that changes per turn.

use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, App, Global, Hsla, component::ActiveTheme as _, div, px, rgb, white};

use crate::entry_view::{EntryContext, timeline_row};

pub const FRAME: Duration = Duration::from_millis(120);

/// Grows then shrinks, so the cycle reads as a pulse.
const GLYPHS: &[&str] = &["·", "✢", "✳", "✶", "✻", "✽", "✻", "✶", "✳", "✢"];

const VERBS: &[&str] = &[
    "Pondering",
    "Thinking",
    "Working",
    "Brewing",
    "Mulling",
    "Tinkering",
    "Crafting",
    "Percolating",
    "Simmering",
    "Noodling",
];

/// The brand color of the harness the user works with, set by the window
/// that shows it: Vortex follows Claude's orange, or for Codex the theme's
/// own text color, white in a dark theme, as Codex's monochrome look.
/// Before a chat picks its harness, Vortex is in its own graphite, as its
/// mark, and no harness's color.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Brand {
    #[default]
    Vortex,
    Claude,
    Codex,
}

impl Global for Brand {}

impl Brand {
    /// The brand of harness `name`, as `AgentBackend::name` gives it.
    #[must_use]
    pub fn of(name: &str) -> Self {
        if name == "codex" {
            Self::Codex
        } else {
            Self::Claude
        }
    }

    /// The harness's color, or the one the user gave it.
    #[must_use]
    pub fn color(self, cx: &App) -> Hsla {
        self.chosen(cx)
            .map_or_else(|| self.own_color(cx), |color| rgb(color).into())
    }

    #[must_use]
    pub fn own_color(self, cx: &App) -> Hsla {
        match self {
            Self::Claude => rgb(0xd9_77_57).into(),
            Self::Codex | Self::Vortex => cx.theme().foreground,
        }
    }

    fn chosen(self, cx: &App) -> Option<u32> {
        crate::look::ChatLook::current(cx).color(self)
    }

    /// What is drawn on the brand color.
    #[must_use]
    pub fn foreground(self, cx: &App) -> Hsla {
        if self.chosen(cx).is_some() {
            // Dark on a light color, light on a dark one.
            return if self.color(cx).l > 0.6 {
                gpui_kit::black()
            } else {
                white()
            };
        }
        match self {
            Self::Claude => white(),
            Self::Codex | Self::Vortex => cx.theme().background,
        }
    }

    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
            Self::Vortex => "Vortex",
        }
    }

    fn current(cx: &App) -> Self {
        cx.try_global::<Self>().copied().unwrap_or_default()
    }
}

/// The brand color, used for the spinner and the send button.
pub fn accent(cx: &App) -> Hsla {
    Brand::current(cx).color(cx)
}

/// The color of an icon or text on [`accent`].
pub fn accent_foreground(cx: &App) -> Hsla {
    Brand::current(cx).foreground(cx)
}

pub fn render(frame: usize, turn: usize, context: &EntryContext, cx: &App) -> AnyElement {
    let glyph = GLYPHS[frame % GLYPHS.len()];
    let verb = VERBS[turn % VERBS.len()];
    // The glyph replaces the dot, so the row is drawn with a transparent one.
    timeline_row(gpui_kit::transparent_black(), context, cx)
        .child(
            div()
                .absolute()
                .left(px(-3.))
                .top_0()
                .w(px(14.))
                .text_center()
                .text_color(accent(cx))
                .child(glyph),
        )
        .child(div().child(format!("{verb}…")))
        .into_any_element()
}
