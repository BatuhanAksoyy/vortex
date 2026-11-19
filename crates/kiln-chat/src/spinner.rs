//! The "✻ Pondering…" row shown while the agent works: a cycling glyph in
//! the timeline's dot column and a verb that changes per turn.

use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, App, Hsla, div, px, rgb};

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

/// Claude's brand orange, used for the spinner and the send button.
pub fn accent() -> Hsla {
    rgb(0xd9_77_57).into()
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
                .text_color(accent())
                .child(glyph),
        )
        .child(div().child(format!("{verb}…")))
        .into_any_element()
}
