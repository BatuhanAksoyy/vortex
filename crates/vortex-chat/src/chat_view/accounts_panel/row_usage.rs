//! An account's plan usage under its name in the accounts panel, as it was
//! last reported, so the user sees what is left before moving to it. One
//! bar holds both windows: the session in the full tone, the longer window
//! in a lighter one. As in the usage panel, the color says only how near
//! the limit each is.

use std::cmp::Reverse;
use std::time::SystemTime;

use gpui_kit::prelude::*;
use gpui_kit::{
    App, Hsla, SharedString, TestSupportExt as _,
    component::{ActiveTheme, Colorize as _, tooltip::Tooltip, v_flex},
    div, px, relative,
};
use vortex_agent::{Usage, UsageWindow};

use super::super::header::{level_color, long_name};
use crate::{clock, scale::ScaledText as _};

const NOT_KNOWN: &str = "No usage reported yet";
/// How much of its own color the longer window's fill keeps; the rest is
/// the track's.
const LIGHTER: f32 = 0.5;

/// What an added account last reported of its usage, and when.
#[derive(Clone)]
pub(super) struct RowUsage {
    pub(super) usage: Usage,
    pub(super) reported_at: Option<SystemTime>,
}

/// Both windows in one bar; the limit instead while it is reached. `None`
/// shows that nothing is known yet.
pub(super) fn render(id: &str, usage: Option<&RowUsage>, cx: &App) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let now = SystemTime::now();
    let lane = v_flex()
        .id(SharedString::from(format!("chat-account-usage-{id}")))
        .test_support()
        .mt_1()
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
    let used = windows(&usage.usage)
        .map(|(name, window)| format!("{name} {}% used", percent(window)))
        .collect::<Vec<_>>()
        .join(" · ");
    lane.aria_label(SharedString::from(used))
        .child(bar(&usage.usage, cx))
}

fn known(usage: &Usage) -> bool {
    usage.session.is_some() || usage.weekly.is_some() || usage.limited.is_some()
}

/// The windows reported, the session first.
fn windows(usage: &Usage) -> impl Iterator<Item = (&'static str, UsageWindow)> {
    [
        usage.session.map(|window| ("Session", window)),
        (usage.weekly).map(|window| (long_name(usage), window)),
    ]
    .into_iter()
    .flatten()
}

fn percent(window: UsageWindow) -> u8 {
    window.percent.min(100)
}

/// The track with a fill for each window. The longer fill lies under the
/// shorter, so both show whichever window is further along.
fn bar(usage: &Usage, cx: &App) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let fills = [
        // Before the session's, so the session's is on top when they are
        // equal.
        (usage.weekly).map(|window| {
            let percent = percent(window);
            (percent, level_color(percent, cx).mix(theme.muted, LIGHTER))
        }),
        usage.session.map(|window| {
            let percent = percent(window);
            (percent, level_color(percent, cx))
        }),
    ];
    let mut fills: Vec<(u8, Hsla)> = fills.into_iter().flatten().collect();
    fills.sort_by_key(|(percent, _)| Reverse(*percent));
    div()
        .relative()
        .h(px(4.))
        .w_full()
        .overflow_hidden()
        .rounded_full()
        .bg(theme.muted)
        .children(fills.into_iter().map(|(percent, color)| {
            div()
                .absolute()
                .top_0()
                .left_0()
                .h_full()
                .rounded_full()
                .bg(color)
                .w(relative(f32::from(percent) / 100.))
        }))
}

/// How far along each window is, when it resets and when the numbers were
/// reported, for the tooltip: an account not in use reports only when
/// asked.
fn details(usage: &RowUsage, now: SystemTime) -> String {
    let windows = windows(&usage.usage).map(|(name, window)| {
        let used = format!("{name} {}% used", percent(window));
        match window.resets_at {
            Some(at) => format!("{used} · resets {}", clock::reset_label(at, now)),
            None => used,
        }
    });
    let updated = (usage.reported_at).map(|at| format!("Updated {}", clock::reset_label(at, now)));
    windows.chain(updated).collect::<Vec<_>>().join("\n")
}
