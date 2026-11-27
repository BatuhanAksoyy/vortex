//! An account's plan usage under its name in the accounts panel, as it was
//! last reported, so the user sees what is left before moving to it. Drawn
//! like the usage panel: the same names, and the bar's color says only how
//! near the limit it is.

use std::time::SystemTime;

use gpui_kit::prelude::*;
use gpui_kit::{
    App, SharedString, TestSupportExt as _,
    component::{ActiveTheme, h_flex, tooltip::Tooltip, v_flex},
    div, px, relative,
};
use vortex_agent::{Usage, UsageWindow};

use super::super::header::{level_color, long_name};
use crate::{clock, scale::ScaledText as _};

const NOT_KNOWN: &str = "No usage reported yet";

/// What an added account last reported of its usage, and when.
#[derive(Clone)]
pub(super) struct RowUsage {
    pub(super) usage: Usage,
    pub(super) reported_at: Option<SystemTime>,
}

/// Each window as a column, its name and percent over a bar; the limit
/// instead while it is reached. `None` shows that nothing is known yet.
pub(super) fn render(id: &str, usage: Option<&RowUsage>, cx: &App) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let now = SystemTime::now();
    let lane = h_flex()
        .id(SharedString::from(format!("chat-account-usage-{id}")))
        .test_support()
        .mt_1()
        .gap_3()
        .text_caption()
        .text_color(theme.muted_foreground);
    let Some(usage) = usage.filter(|usage| known(&usage.usage)) else {
        return lane.aria_label(NOT_KNOWN).child(NOT_KNOWN);
    };
    let limited = (usage.usage.limited)
        .filter(|limited| limited.until.is_none_or(|until| until > now))
        .map(|limited| match limited.until {
            Some(until) => format!("Limit reached · resets {}", clock::reset_label(until, now)),
            None => "Limit reached".to_owned(),
        });
    let details = SharedString::from(details(usage, now));
    let lane = lane.tooltip(move |window, cx| Tooltip::new(details.clone()).build(window, cx));
    if let Some(text) = limited {
        return lane
            .aria_label(SharedString::from(text.clone()))
            .text_color(theme.danger)
            .child(text);
    }
    let windows = [
        usage.usage.session.map(|window| ("Session", window)),
        (usage.usage.weekly).map(|window| (long_name(&usage.usage), window)),
    ];
    lane.children(
        windows
            .into_iter()
            .flatten()
            .map(|(name, window)| column(name, window, cx)),
    )
}

fn known(usage: &Usage) -> bool {
    usage.session.is_some() || usage.weekly.is_some() || usage.limited.is_some()
}

/// A window's name and percent over the bar, each window as wide as the
/// other so the bars line up from one account to the next.
fn column(name: &'static str, window: UsageWindow, cx: &App) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let percent = window.percent.min(100);
    v_flex()
        .flex_1()
        .min_w_0()
        .gap_0p5()
        // Under the lane's id, so each account's are apart.
        .id(name)
        .test_support()
        .aria_label(SharedString::from(format!("{name} {percent}% used")))
        .child(
            h_flex().justify_between().child(name).child(
                div()
                    .text_color(theme.foreground)
                    .child(format!("{percent}%")),
            ),
        )
        .child(
            div()
                .h(px(3.))
                .w_full()
                .overflow_hidden()
                .rounded_full()
                .bg(theme.muted)
                .child(
                    div()
                        .h_full()
                        .rounded_full()
                        .bg(level_color(percent, cx))
                        .w(relative(f32::from(percent) / 100.)),
                ),
        )
}

/// When each window resets and when the numbers were reported, for the
/// tooltip: an account not in use reports only when asked.
fn details(usage: &RowUsage, now: SystemTime) -> String {
    let resets = |name: &str, window: Option<UsageWindow>| {
        let at = window?.resets_at?;
        Some(format!("{name} resets {}", clock::reset_label(at, now)))
    };
    let lines = [
        resets("Session", usage.usage.session),
        resets(long_name(&usage.usage), usage.usage.weekly),
        (usage.reported_at).map(|at| format!("Updated {}", clock::reset_label(at, now))),
    ];
    lines.into_iter().flatten().collect::<Vec<_>>().join("\n")
}
