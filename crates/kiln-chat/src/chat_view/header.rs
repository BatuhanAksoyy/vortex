//! The title bar: the conversation's name, the account the agent is signed
//! in with, and how much of the subscription's usage is spent.

use std::time::SystemTime;

use gpui_kit::prelude::*;
use gpui_kit::{
    Anchor, Context, Hsla, TestSupportExt, WeakEntity,
    component::{
        ActiveTheme, Icon, IconName, Sizable,
        button::{Button, ButtonVariants},
        h_flex,
        popover::Popover,
        v_flex,
    },
    div, px, relative,
};
use kiln_agent::{Usage, UsageWindow};

use super::{ChatView, StripPanel};

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
            .child(self.account_button(cx))
            .child(usage_meter(
                self.agent.usage(),
                self.agent.usage_updated_at(),
                self.is_open(StripPanel::Usage),
                cx.entity().downgrade(),
                cx,
            ))
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

/// The usage button and its panel: the session window's percent, and on a
/// click fresh numbers for both windows with their reset times. Shown before
/// any numbers came, so `/usage` always has a panel to open; its open state
/// lives in the view for that reason.
fn usage_meter(
    usage: Usage,
    updated: Option<SystemTime>,
    open: bool,
    view: WeakEntity<ChatView>,
    cx: &gpui_kit::App,
) -> impl IntoElement + use<> {
    let (label, color) = match usage.session {
        Some(session) => (
            format!("{}% used", session.percent),
            level_color(session.percent, cx),
        ),
        None => ("Usage".to_owned(), cx.theme().muted_foreground),
    };
    let trigger = Button::new("chat-usage")
        .ghost()
        .xsmall()
        .label(label)
        .text_color(color)
        .tooltip("Plan usage");
    Popover::new("chat-usage-popover")
        .anchor(Anchor::TopRight)
        .trigger(trigger)
        .open(open)
        .on_open_change(move |open, _, cx| {
            let _ = view.update(cx, |view, cx| view.set_usage_open(*open, cx));
        })
        .content(move |_, _, cx| usage_panel(usage, updated, cx))
}

fn usage_panel(
    usage: Usage,
    updated: Option<SystemTime>,
    cx: &gpui_kit::App,
) -> impl IntoElement + use<> {
    let now = SystemTime::now();
    let updated = updated.map(|at| format!("Updated {}", reset_label(at, now)));
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
        .when(usage.session.is_none() && usage.weekly.is_none(), |this| {
            this.child(
                div()
                    .id("chat-usage-empty")
                    .test_support()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("No usage reported for this account yet."),
            )
        })
        .children(updated.map(|text| {
            div()
                .id("chat-usage-updated")
                .test_support()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(text)
        }))
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
