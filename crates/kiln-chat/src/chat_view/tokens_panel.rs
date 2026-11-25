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
use kiln_agent::SpentTokens;

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
                .content(move |_, _, cx| panel(spent, cx)),
        )
    }
}

fn panel(spent: SpentTokens, cx: &App) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let row = |label: &'static str, count: u64| {
        h_flex().justify_between().child(label).child(tokens(count))
    };
    let total = tokens(spent.total());
    v_flex()
        .id("chat-tokens-panel")
        .test_support()
        .aria_label(SharedString::from(format!("{total} tokens spent")))
        .w(px(PANEL_WIDTH))
        .gap_2()
        .text_sm()
        .child(h_flex().justify_between().child("Tokens spent").child(total))
        .child(div().h_px().bg(theme.border))
        .child(row("Output, with thinking", spent.output))
        .child(row("Input, not cached", spent.input))
        .child(row("Written to cache", spent.written))
        .child(row("Read from cache", spent.cached))
        .child(
            div()
                .text_caption()
                .text_color(theme.muted_foreground)
                .child("Every request sends the conversation again, mostly read from the cache. Subagents are added when the turn ends."),
        )
}
