//! The context meter in the composer strip and the panel it opens: how full
//! the context window is and what fills it, as the agent breaks it down,
//! each part in its own color on the bar and beside its row. `/context`
//! opens the same panel.

use gpui_kit::prelude::*;
use gpui_kit::{
    Anchor, Context, Hsla, TestSupportExt,
    component::{
        ActiveTheme,
        button::{Button, ButtonVariants},
        h_flex,
        popover::Popover,
        v_flex,
    },
    div, px, relative,
};
use vortex_agent::{ContextCategory, ContextKind, ContextUsage};

use super::{ChatView, StripPanel};
use crate::{
    agent_state::tokens,
    scale::{ScaledButton as _, ScaledText as _},
};

const PANEL_WIDTH: f32 = 300.;

impl ChatView {
    pub(super) fn context_meter(&self, cx: &Context<Self>) -> Option<impl IntoElement + use<>> {
        let percent = self.agent.context_percent()?;
        let usage = self.agent.context()?.clone();
        let color = level_color(percent, cx);
        let view = cx.entity().downgrade();
        let trigger = Button::new("chat-context")
            .ghost()
            .control()
            .px_1p5()
            .label(format!("{percent}%"))
            .text_color(color)
            .tooltip("Context usage");
        Some(
            Popover::new("chat-context-popover")
                .anchor(Anchor::BottomRight)
                .trigger(trigger)
                .open(self.is_open(StripPanel::Context))
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
    let palette = palette(cx);
    let used: Vec<(&ContextCategory, Hsla)> = of_kind(ContextKind::Used)
        .zip(palette.iter().copied().cycle())
        .collect();
    let reserved: Vec<_> = of_kind(ContextKind::Buffer)
        .chain(of_kind(ContextKind::Free))
        .map(|category| category_row(category, None, cx))
        .collect();
    let deferred: Vec<_> = of_kind(ContextKind::Deferred)
        .map(|category| category_row(category, None, cx))
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
        .child(bar(&used, usage, percent, cx))
        .children(
            used.iter()
                .map(|(category, color)| category_row(category, Some(*color), cx)),
        )
        .when(!reserved.is_empty(), |this| {
            this.child(div().h_px().bg(theme.border)).children(reserved)
        })
        .when(!deferred.is_empty(), |this| {
            this.child(
                div()
                    .pt_1()
                    .text_caption()
                    .text_color(theme.muted_foreground)
                    .child("Loaded on demand, not counted"),
            )
            .children(deferred)
        })
}

/// The window's fill, a segment per part in that part's color. Without a
/// breakdown it fills as one, in the meter's color.
fn bar(
    used: &[(&ContextCategory, Hsla)],
    usage: &ContextUsage,
    percent: u64,
    cx: &gpui_kit::App,
) -> impl IntoElement + use<> {
    // In thousandths, which the bar draws as finely as it can.
    let share = |tokens: u64| {
        let thousandths = tokens.saturating_mul(1000) / usage.max_tokens.max(1);
        relative(f32::from(u16::try_from(thousandths.min(1000)).unwrap_or(1000)) / 1000.)
    };
    let segments: Vec<(gpui_kit::DefiniteLength, Hsla)> = if used.is_empty() {
        let fill = relative(f32::from(u8::try_from(percent.min(100)).unwrap_or(100)) / 100.);
        vec![(fill, level_color(percent, cx))]
    } else {
        (used.iter())
            .map(|(category, color)| (share(category.tokens), *color))
            .collect()
    };
    h_flex()
        .h(px(6.))
        .w_full()
        .gap(px(1.))
        .overflow_hidden()
        .rounded_full()
        .bg(cx.theme().muted)
        .children(
            (segments.into_iter())
                .map(|(width, color)| div().flex_none().h_full().w(width).bg(color)),
        )
}

/// A part's name and size, after a dot in its color on the bar; `None`
/// for parts the bar leaves out, which are muted.
fn category_row(
    category: &ContextCategory,
    color: Option<Hsla>,
    cx: &gpui_kit::App,
) -> impl IntoElement + use<> {
    let muted = cx.theme().muted_foreground;
    h_flex()
        .gap_2()
        .when(color.is_none(), |this| this.text_color(muted))
        .child(
            div()
                .flex_none()
                .size(px(8.))
                .rounded_full()
                .bg(color.unwrap_or(cx.theme().muted)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .child(category.name.clone()),
        )
        .child(tokens(category.tokens))
}

/// Hues far apart, from the theme's base colors, so neighbours on the bar
/// are told apart; its chart colors are shades of one hue in some themes.
fn palette(cx: &gpui_kit::App) -> [Hsla; 8] {
    let theme = cx.theme();
    [
        theme.blue,
        theme.green,
        theme.yellow,
        theme.magenta,
        theme.cyan,
        theme.red,
        theme.blue_light,
        theme.yellow_light,
    ]
}

fn level_color(percent: u64, cx: &gpui_kit::App) -> Hsla {
    match percent {
        0..70 => cx.theme().muted_foreground,
        70..90 => cx.theme().warning,
        _ => cx.theme().danger,
    }
}
