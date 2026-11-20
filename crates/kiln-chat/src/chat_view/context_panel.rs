//! The context meter in the composer strip and the panel it opens: how full
//! the context window is and what fills it, as the agent breaks it down.
//! `/context` opens the same panel.

use gpui_kit::prelude::*;
use gpui_kit::{
    Anchor, Context, Hsla, TestSupportExt,
    component::{
        ActiveTheme, Sizable,
        button::{Button, ButtonVariants},
        h_flex,
        popover::Popover,
        v_flex,
    },
    div, px, relative,
};
use kiln_agent::{ContextCategory, ContextKind, ContextUsage};

use super::ChatView;
use crate::agent_state::tokens;

const PANEL_WIDTH: f32 = 300.;

impl ChatView {
    pub(super) fn context_meter(&self, cx: &Context<Self>) -> Option<impl IntoElement + use<>> {
        let percent = self.agent.context_percent()?;
        let usage = self.agent.context()?.clone();
        let color = level_color(percent, cx);
        let view = cx.entity().downgrade();
        let trigger = Button::new("chat-context")
            .ghost()
            .xsmall()
            .label(format!("{percent}%"))
            .text_color(color)
            .tooltip("Context usage");
        Some(
            Popover::new("chat-context-popover")
                .anchor(Anchor::BottomRight)
                .trigger(trigger)
                .open(self.context_open)
                .on_open_change(move |open, _, cx| {
                    let _ = view.update(cx, |view, cx| view.set_context_open(*open, cx));
                })
                .content(move |_, _, cx| context_panel(&usage, percent, cx)),
        )
    }
}

fn context_panel(
    usage: &ContextUsage,
    percent: u64,
    cx: &gpui_kit::App,
) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let color = level_color(percent, cx);
    let of_kind = |kind: ContextKind| {
        usage
            .categories
            .iter()
            .filter(move |category| category.kind == kind)
    };
    let fill = relative(f32::from(u8::try_from(percent.min(100)).unwrap_or(100)) / 100.);
    let reserved: Vec<_> = of_kind(ContextKind::Buffer)
        .chain(of_kind(ContextKind::Free))
        .map(|category| category_row(category, true, cx))
        .collect();
    let deferred: Vec<_> = of_kind(ContextKind::Deferred)
        .map(|category| category_row(category, true, cx))
        .collect();
    v_flex()
        .id("chat-context-panel")
        .test_support()
        .w(px(PANEL_WIDTH))
        .gap_2()
        .text_sm()
        .child(h_flex().justify_between().child("Context window").child(
            div().text_color(color).child(format!(
                "{} of {} · {percent}%",
                tokens(usage.used_tokens),
                tokens(usage.max_tokens)
            )),
        ))
        .child(
            div()
                .h(px(6.))
                .w_full()
                .rounded_full()
                .bg(theme.muted)
                .child(div().h_full().rounded_full().bg(color).w(fill)),
        )
        .children(of_kind(ContextKind::Used).map(|category| category_row(category, false, cx)))
        .when(!reserved.is_empty(), |this| {
            this.child(div().h_px().bg(theme.border)).children(reserved)
        })
        .when(!deferred.is_empty(), |this| {
            this.child(
                div()
                    .pt_1()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("Loaded on demand, not counted"),
            )
            .children(deferred)
        })
}

fn category_row(
    category: &ContextCategory,
    muted: bool,
    cx: &gpui_kit::App,
) -> impl IntoElement + use<> {
    h_flex()
        .justify_between()
        .when(muted, |this| this.text_color(cx.theme().muted_foreground))
        .child(category.name.clone())
        .child(tokens(category.tokens))
}

fn level_color(percent: u64, cx: &gpui_kit::App) -> Hsla {
    match percent {
        0..70 => cx.theme().muted_foreground,
        70..90 => cx.theme().warning,
        _ => cx.theme().danger,
    }
}
