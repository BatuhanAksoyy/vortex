//! An account's plan usage under its name in the accounts panel, as it was
//! last reported, so the user sees what is left before moving to it. One
//! bar holds both windows: the session in the full tone, the longer window
//! in a lighter one, with when each one resets under it.
//! As in the usage panel, the color says only how near the limit each is.

use std::cmp::Reverse;
use std::time::SystemTime;

use gpui_kit::prelude::*;
use gpui_kit::{
    App, Hsla, SharedString, TestSupportExt as _,
    component::{ActiveTheme, Colorize as _, h_flex, tooltip::Tooltip, v_flex},
    div, px, relative,
};
use vortex_agent::{Usage, UsageWindow};

use super::super::header::{level_color, long_name};
use crate::{clock, scale::ScaledText as _};

const NOT_KNOWN: &str = "No usage reported yet";
/// How much of its own color the longer window's fill keeps; the rest is
/// the track's.
const LIGHTER: f32 = 0.5;
const DOT: f32 = 6.;

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
    let fills = fills(&usage.usage, cx);
    lane.aria_label(SharedString::from(used))
        .child(bar(&fills, cx))
        .children(legend(&usage.usage, &fills, now, cx))
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

/// Each window's percent and the color of its fill: the session in its
/// level's full tone, the longer window in a lighter one. The session first.
fn fills(usage: &Usage, cx: &App) -> Vec<(u8, Hsla)> {
    let lighter = |color: Hsla| color.mix(cx.theme().muted, LIGHTER);
    let fills = [
        usage.session.map(|window| (percent(window), false)),
        (usage.weekly).map(|window| (percent(window), true)),
    ];
    fills
        .into_iter()
        .flatten()
        .map(|(percent, light)| {
            let color = level_color(percent, cx);
            (percent, if light { lighter(color) } else { color })
        })
        .collect()
}

/// The track with a fill for each window. The longer fill lies under the
/// shorter, so both show whichever window is further along.
fn bar(fills: &[(u8, Hsla)], cx: &App) -> impl IntoElement + use<> {
    // The session's last, so it is on top when they are equal.
    let mut drawn: Vec<(u8, Hsla)> = fills.iter().rev().copied().collect();
    drawn.sort_by_key(|(percent, _)| Reverse(*percent));
    div()
        .relative()
        .h(px(4.))
        .w_full()
        .overflow_hidden()
        .rounded_full()
        .bg(cx.theme().muted)
        .children(drawn.into_iter().map(|(percent, color)| {
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

/// Under the bar: when each window resets, by a dot of its fill's color,
/// which names it without a word. Nothing when no reset is known.
fn legend(
    usage: &Usage,
    fills: &[(u8, Hsla)],
    now: SystemTime,
    cx: &App,
) -> Option<impl IntoElement + use<>> {
    let foreground = cx.theme().foreground;
    // `fills` is in the windows' order.
    let resets: Vec<(&str, Hsla, String)> = (windows(usage).zip(fills))
        .filter_map(|((name, window), &(_, color))| {
            let at = window.resets_at.filter(|at| *at > now)?;
            Some((name, color, clock::reset_label(at, now)))
        })
        .collect();
    let said = (resets.iter())
        .map(|(name, _, at)| format!("{name} resets {at}"))
        .collect::<Vec<_>>()
        .join(" · ");
    (!resets.is_empty()).then(|| {
        h_flex()
            // Under the lane's id, so each account's are apart.
            .id("resets")
            .test_support()
            .aria_label(SharedString::from(said))
            .mt_1()
            .gap_3()
            .children(resets.into_iter().map(|(_, color, at)| {
                h_flex()
                    .gap_1()
                    .child(div().flex_none().size(px(DOT)).rounded_full().bg(color))
                    .child(div().text_color(foreground).child(at))
            }))
    })
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
