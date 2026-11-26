//! The "✻ Pondering…" row shown while the agent works: a cycling glyph in
//! the timeline's dot column and a verb that changes per turn.

use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, App, Global, Hsla, div, px, rgb};

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
/// that shows it: Kiln follows Claude's orange or Codex's blue.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Brand {
    #[default]
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

    #[must_use]
    pub fn color(self) -> Hsla {
        match self {
            Self::Claude => rgb(0xd9_77_57).into(),
            Self::Codex => rgb(0x3b_82_f6).into(),
        }
    }
}

/// The brand color, used for the spinner and the send button.
pub fn accent(cx: &App) -> Hsla {
    cx.try_global::<Brand>()
        .copied()
        .unwrap_or_default()
        .color()
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
