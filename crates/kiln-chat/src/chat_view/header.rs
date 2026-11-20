//! The title bar: the conversation's name, the account the agent is signed
//! in with, and how much of the subscription's usage is spent.

use std::time::SystemTime;

use gpui_kit::prelude::*;
use gpui_kit::{
    Anchor, Context, Hsla, SharedString, TestSupportExt,
    component::{
        ActiveTheme, Icon, IconName, Sizable,
        button::{Button, ButtonVariants},
        h_flex,
        popover::Popover,
        tooltip::Tooltip,
        v_flex,
    },
    div, px, relative,
};
use kiln_agent::{Account, Usage, UsageWindow};

use super::ChatView;

const USAGE_PANEL_WIDTH: f32 = 260.;

impl ChatView {
    pub(super) fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let title = self
            .transcript
            .title()
            .unwrap_or("New conversation")
            .to_owned();
        h_flex()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(theme.border)
            .child(div().flex_1().min_w_0().truncate().child(title))
            .children(
                self.agent
                    .account()
                    .map(|account| account_badge(account, cx)),
            )
            .children(usage_meter(self.agent.usage(), cx))
            .child(
                div()
                    .id("chat-new")
                    .test_support()
                    .aria_label("New conversation")
                    .p_1()
                    .rounded(theme.radius)
                    .cursor_pointer()
                    .hover(|style| style.bg(gpui_kit::black().opacity(0.08)))
                    .child(Icon::new(IconName::Plus).small())
                    .on_click(cx.listener(|this, _, window, cx| this.new_conversation(window, cx))),
            )
    }
}

/// The plan, e.g. "Pro"; the address and organization show on hover.
fn account_badge(account: &Account, cx: &gpui_kit::App) -> impl IntoElement + use<> {
    let plan = account
        .plan
        .strip_prefix("Claude ")
        .unwrap_or(&account.plan);
    let plan = if plan.is_empty() { "Account" } else { plan };
    let details: SharedString = [account.email.as_str(), account.organization.as_str()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
        .into();
    let theme = cx.theme();
    div()
        .id("chat-account")
        .test_support()
        .aria_label(SharedString::from(format!("Account {plan}")))
        .px_1p5()
        .rounded(theme.radius)
        .border_1()
        .border_color(theme.border)
        .text_xs()
        .text_color(theme.muted_foreground)
        .child(plan.to_owned())
        .when(!details.is_empty(), |this| {
            this.tooltip(move |window, cx| Tooltip::new(details.clone()).build(window, cx))
        })
}

/// The session window's percent; a click shows both windows and their
/// reset times.
fn usage_meter(usage: Usage, cx: &gpui_kit::App) -> Option<impl IntoElement + use<>> {
    let session = usage.session?;
    let color = level_color(session.percent, cx);
    let trigger = Button::new("chat-usage")
        .ghost()
        .xsmall()
        .label(format!("{}% used", session.percent))
        .text_color(color)
        .tooltip("Plan usage");
    Some(
        Popover::new("chat-usage-popover")
            .anchor(Anchor::TopRight)
            .trigger(trigger)
            .content(move |_, _, cx| usage_panel(usage, cx)),
    )
}

fn usage_panel(usage: Usage, cx: &gpui_kit::App) -> impl IntoElement + use<> {
    let now = SystemTime::now();
    v_flex()
        .id("chat-usage-panel")
        .test_support()
        .w(px(USAGE_PANEL_WIDTH))
        .gap_3()
        .children(
            usage
                .session
                .map(|w| usage_row("Current session", w, now, cx)),
        )
        .children(usage.weekly.map(|w| usage_row("Weekly", w, now, cx)))
}

fn usage_row(
    name: &'static str,
    window: UsageWindow,
    now: SystemTime,
    cx: &gpui_kit::App,
) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let color = level_color(window.percent, cx);
    let resets = window
        .resets_at
        .map(|at| format!("Resets {}", reset_label(at, now)));
    v_flex()
        .gap_1()
        .text_sm()
        .child(
            h_flex().justify_between().child(name).child(
                div()
                    .text_color(color)
                    .child(format!("{}% used", window.percent)),
            ),
        )
        .child(
            div()
                .h(px(6.))
                .w_full()
                .rounded_full()
                .bg(theme.muted)
                .child(
                    div()
                        .h_full()
                        .rounded_full()
                        .bg(color)
                        .w(relative(f32::from(window.percent) / 100.)),
                ),
        )
        .children(resets.map(|text| {
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(text)
        }))
}

fn level_color(percent: u8, cx: &gpui_kit::App) -> Hsla {
    match percent {
        0..80 => cx.theme().muted_foreground,
        80..100 => cx.theme().warning,
        _ => cx.theme().danger,
    }
}

/// "15:00" today, "Mon 11:00" on another day, in local time.
pub(super) fn reset_label(at: SystemTime, now: SystemTime) -> String {
    let local = |time: SystemTime| {
        jiff::Timestamp::try_from(time)
            .unwrap_or(jiff::Timestamp::UNIX_EPOCH)
            .to_zoned(jiff::tz::TimeZone::system())
    };
    let (at, now) = (local(at), local(now));
    if at.date() == now.date() {
        at.strftime("%H:%M").to_string()
    } else {
        at.strftime("%a %H:%M").to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn a_reset_on_another_day_names_the_day() {
        let now = SystemTime::now();
        assert_eq!(reset_label(now, now).len(), "15:00".len());
        let next_week = now + Duration::from_hours(72);
        assert!(reset_label(next_week, now).contains(' '));
    }
}
