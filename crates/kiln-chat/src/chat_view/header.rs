//! The title bar: the conversation's name, the account the agent is signed
//! in with, and how much of the subscription's usage is spent.

use std::time::SystemTime;

use gpui_kit::prelude::*;
use gpui_kit::{
    Anchor, Context, Hsla, TestSupportExt, WeakEntity,
    component::{
        ActiveTheme, Icon, IconName,
        button::{Button, ButtonVariants},
        h_flex,
        popover::Popover,
        v_flex,
    },
    div, px, relative,
};
use kiln_agent::{Usage, UsageWindow};

use super::{ChatView, StripPanel};
use crate::{
    clock,
    scale::{ScaledButton as _, ScaledIcon as _, ScaledText as _},
};

const USAGE_PANEL_WIDTH: f32 = 260.;

impl ChatView {
    pub(super) fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let title = self.title().unwrap_or("New conversation").to_owned();
        h_flex()
            .gap_1()
            // So the project's icon starts where the transcript does.
            .pl_5()
            .pr_3()
            .py_1p5()
            .border_b_1()
            .border_color(theme.border)
            .child(self.project_button(cx))
            .child(
                div().flex_1().min_w_0().child(
                    h_flex()
                        .id("chat-title")
                        .test_support()
                        .aria_label(gpui_kit::SharedString::from(format!(
                            "Conversation: {title}"
                        )))
                        .max_w_full()
                        .gap_1()
                        .px_1()
                        .rounded(theme.radius)
                        .cursor_pointer()
                        .hover(|style| style.bg(gpui_kit::black().opacity(0.08)))
                        .tooltip(|window, cx| {
                            gpui_kit::component::tooltip::Tooltip::new("Switch conversation")
                                .build(window, cx)
                        })
                        .on_click(cx.listener(|_, _, _, cx| {
                            cx.emit(super::ChatEvent::PickConversation);
                        }))
                        .child(div().min_w_0().truncate().child(title))
                        .child(
                            Icon::new(gpui_kit::assets::IconName::ChevronDown)
                                .in_row()
                                .text_color(theme.muted_foreground),
                        ),
                ),
            )
            .children(self.process_meter.clone())
            .child(self.account_button(cx))
            .child(usage_meter(
                self.agent.usage(),
                self.agent.usage_updated_at(),
                self.is_open(StripPanel::Usage),
                cx.entity().downgrade(),
                cx,
            ))
            .child(
                Button::new("chat-new")
                    .ghost()
                    .control_icon(IconName::Plus)
                    .accessibility_label("New conversation")
                    .tooltip("New conversation")
                    .on_click(cx.listener(|this, _, window, cx| this.new_conversation(window, cx))),
            )
    }
}

impl ChatView {
    /// The folder the agent works in, which opens the projects page.
    fn project_button(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let name = self.cwd.file_name().unwrap_or(self.cwd.as_os_str());
        Button::new("chat-project")
            .ghost()
            .control_labelled(
                gpui_kit::assets::IconName::FolderOpen,
                name.to_string_lossy().into_owned(),
            )
            .flex_none()
            .tooltip(self.cwd.display().to_string())
            .on_click(cx.listener(|_, _, _, cx| cx.emit(super::ChatEvent::PickProject)))
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
        .control()
        .px_1p5()
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
    let updated = updated.map(|at| format!("Updated {}", clock::reset_label(at, now)));
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
                .text_caption()
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
        .map(|at| format!("Resets {}", clock::reset_label(at, now)));
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
                .text_caption()
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
