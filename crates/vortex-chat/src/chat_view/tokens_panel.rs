//! The total of tokens the conversation spent, in the composer strip, and
//! the panel it opens, which says on what.

use gpui_kit::prelude::*;
use gpui_kit::{
    Anchor, App, Context, SharedString, TestSupportExt,
    component::{
        ActiveTheme,
        button::{Button, ButtonVariants},
        h_flex,
        popover::Popover,
        v_flex,
    },
    div, px,
};
use vortex_agent::SpentTokens;

use super::{ChatView, StripPanel};
use crate::{
    agent_state::tokens,
    scale::{ScaledButton as _, ScaledText as _},
};

const PANEL_WIDTH: f32 = 300.;

impl ChatView {
    /// Shown once anything is spent.
    pub(super) fn tokens_button(&self, cx: &Context<Self>) -> Option<impl IntoElement + use<>> {
        let spent = self.agent.spent()?;
        let cache = self.agent.prompt_cache();
        let requests = (cache.requests(), cache.average_prompt());
        let view = cx.entity().downgrade();
        let trigger = Button::new("chat-tokens")
            .ghost()
            .control()
            .px_1p5()
            .label(format!("{} tokens", tokens(spent.total())))
            .text_color(cx.theme().muted_foreground)
            .tooltip("Tokens spent in this conversation");
        Some(
            Popover::new("chat-tokens-popover")
                .anchor(Anchor::BottomRight)
                .trigger(trigger)
                .open(self.is_open(StripPanel::Tokens))
                .on_open_change(move |open, _, cx| {
                    let _ = view.update(cx, |view, cx| {
                        view.show_panel(StripPanel::Tokens, *open);
                        cx.notify();
                    });
                })
                .content(move |_, _, cx| panel(spent, requests, cx)),
        )
    }
}

/// `requests` is how many the conversation made and the prompt an average
/// one sent, which is what makes a few short messages add up: each sends
/// the whole prompt again.
fn panel(
    spent: SpentTokens,
    (requests, average): (u64, Option<u64>),
    cx: &App,
) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let row = |label: &'static str, count: u64| {
        h_flex().justify_between().child(label).child(tokens(count))
    };
    // What the new tokens are made of, under their sum.
    let part = |label, count| row(label, count).pl_3().text_color(theme.muted_foreground);
    let total = tokens(spent.total());
    let per_request = average.map(|average| {
        let count = if requests == 1 { "request" } else { "requests" };
        format!(
            "{requests} {count}, each sending about {} of prompt",
            tokens(average)
        )
    });
    v_flex()
        .id("chat-tokens-panel")
        .test_support()
        .aria_label(SharedString::from(format!("{total} tokens spent")))
        .w(px(PANEL_WIDTH))
        .gap_2()
        .text_sm()
        .child(h_flex().justify_between().child("Tokens spent").child(total))
        .children(per_request.map(|text| {
            div()
                .id("chat-tokens-requests")
                .test_support()
                .aria_label(SharedString::from(text.clone()))
                .text_caption()
                .text_color(theme.muted_foreground)
                .child(text)
        }))
        .child(div().h_px().bg(theme.border))
        .child(row("New, not from the cache", spent.fresh()))
        .child(part("Output, with thinking", spent.output))
        .child(part("Input, not cached", spent.input))
        .child(part("Written to cache", spent.written))
        .child(row("Read from cache", spent.cached))
        .child(
            div()
                .text_caption()
                .text_color(theme.muted_foreground)
                .child("Every request sends the whole prompt again: Claude Code's instructions, the tools and the conversation so far. Most of it is read from the cache, which API prices count at a tenth of new input. Subagents are added when the turn ends."),
        )
}
