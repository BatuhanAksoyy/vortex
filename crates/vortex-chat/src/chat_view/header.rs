//! The title bar: the conversation's name, the account the agent is signed
//! in with, and how much of the subscription's usage is spent.

use std::time::SystemTime;

use gpui_kit::prelude::*;
use gpui_kit::{
    Anchor, Context, Hsla, TestSupportExt, WeakEntity,
    component::{
        ActiveTheme, Icon,
        button::{Button, ButtonVariants},
        h_flex,
        popover::Popover,
        v_flex,
    },
    div, px, relative,
};
use vortex_agent::{Usage, UsageWindow};

mod customize;

use super::{ChatView, StripPanel};
use crate::{
    agent_state::tokens,
    clock,
    look::{ChatLook, Spot, UsageShown},
    scale::{ScaledButton as _, ScaledIcon as _, ScaledText as _},
    tool_format,
    transcript::Entry,
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
            .child(self.render_spot(Spot::Top, cx))
            .child(Self::customize_button(cx))
    }

    fn usage_element(&self, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        usage_meter(
            self.agent.usage(),
            UsageTimes {
                updated: self.agent.usage_updated_at(),
                refused_until: self.usage_refused_until(cx),
                this_chat: self.this_chat(),
            },
            self.is_open(StripPanel::Usage),
            cx.entity().downgrade(),
            cx,
        )
        .into_any_element()
    }
}

impl ChatView {
    /// What this chat did with the plan, for the usage panel.
    fn this_chat(&self) -> ThisChat {
        let mut files = std::collections::HashSet::new();
        for row in 0..self.transcript.len() {
            let Some((_, Entry::Tool { call, result })) = self.transcript.entry(row) else {
                continue;
            };
            let done = result.as_ref().is_some_and(|result| !result.is_error);
            let path = call.input["file_path"]
                .as_str()
                .or(call.input["path"].as_str());
            if done && tool_format::edit_diff(&call.name, &call.input).is_some() {
                files.extend(path);
            }
        }
        ThisChat {
            share: self.agent.session_share(),
            turns: self.transcript.turns(),
            tokens: self.agent.spent(),
            files: files.len(),
        }
    }

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

/// The usage button and its panel: the fuller window's percent, and on a
/// click fresh numbers for both windows with their reset times. Shown before
/// any numbers came, so `/usage` always has a panel to open; its open state
/// lives in the view for that reason.
/// When the usage shown was reported, and until when asking for fresh
/// numbers is refused.
#[derive(Clone, Copy)]
struct UsageTimes {
    updated: Option<SystemTime>,
    refused_until: Option<SystemTime>,
    this_chat: ThisChat,
}

/// What this chat did: about how much of the session window it used, and
/// what for.
#[derive(Clone, Copy)]
struct ThisChat {
    share: Option<u8>,
    turns: usize,
    tokens: Option<vortex_agent::SpentTokens>,
    files: usize,
}

fn usage_meter(
    usage: Usage,
    times: UsageTimes,
    open: bool,
    view: WeakEntity<ChatView>,
    cx: &gpui_kit::App,
) -> impl IntoElement + use<> {
    let (label, color) = match meter_label(&usage, ChatLook::current(cx).usage) {
        Some((label, percent)) => (label, level_color(percent, cx)),
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
        .content(move |_, _, cx| usage_panel(usage, times, cx))
}

/// The window nearer its limit, which is the one that stops the user
/// first; the session one on a tie. A plan may have only one, as Codex's
/// free plan has only the weekly.
pub(crate) fn fuller(usage: &Usage) -> Option<(&'static str, UsageWindow)> {
    let session = usage.session.map(|window| ("Session", window));
    let weekly = usage.weekly.map(|window| (long_name(usage), window));
    match (session, weekly) {
        (Some(session), Some(weekly)) if weekly.1.percent > session.1.percent => Some(weekly),
        (session, weekly) => session.or(weekly),
    }
}

/// What the meter reads with `shown`, and the percent it is colored by.
/// A window the plan lacks falls back to the fuller one.
fn meter_label(usage: &Usage, shown: UsageShown) -> Option<(String, u8)> {
    let named =
        |name: &str, window: UsageWindow| (format!("{name} {}%", window.percent), window.percent);
    let picked = match shown {
        UsageShown::Session => usage.session.map(|window| named("Session", window)),
        UsageShown::Weekly => usage.weekly.map(|window| named(long_name(usage), window)),
        UsageShown::Both => usage.session.zip(usage.weekly).map(|(session, weekly)| {
            let label = format!(
                "Session {}% · {} {}%",
                session.percent,
                long_name(usage),
                weekly.percent
            );
            (label, session.percent.max(weekly.percent))
        }),
        UsageShown::Fuller => None,
    };
    picked.or_else(|| fuller(usage).map(|(name, window)| named(name, window)))
}

/// What the long window is called: weekly, or monthly for a 30-day one.
pub(super) fn long_name(usage: &Usage) -> &'static str {
    match usage.long_window_days {
        Some(28..=31) => "Monthly",
        Some(_) => "Long",
        None => "Weekly",
    }
}

fn usage_panel(usage: Usage, times: UsageTimes, cx: &gpui_kit::App) -> impl IntoElement + use<> {
    let now = SystemTime::now();
    let updated = (times.updated).map(|at| format!("Updated {}", clock::reset_label(at, now)));
    let refused = times.refused_until.map(|until| {
        format!(
            "Claude is not giving this account's usage right now, as it was asked too often. \
             Vortex asks again at {}; replies keep it current meanwhile.",
            clock::reset_label(until, now)
        )
    });
    v_flex()
        .id("chat-usage-panel")
        .test_support()
        .w(px(USAGE_PANEL_WIDTH))
        .gap_3()
        .children(
            usage
                .session
                .map(|w| usage_row("Current session", w, times.this_chat.share, now, cx)),
        )
        .children(
            usage
                .weekly
                .map(|w| usage_row(long_name(&usage), w, None, now, cx)),
        )
        .child(this_chat(times.this_chat, cx))
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
        .children(refused.map(|text| {
            div()
                .id("chat-usage-refused")
                .test_support()
                .text_caption()
                .text_color(cx.theme().warning)
                .child(text)
        }))
}

/// A window's bar; `here`, this chat's part of it, shows at its end in the
/// accent color, after what the account used elsewhere.
fn usage_row(
    name: &'static str,
    window: UsageWindow,
    here: Option<u8>,
    now: SystemTime,
    cx: &gpui_kit::App,
) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let color = level_color(window.percent, cx);
    let resets = window
        .resets_at
        .map(|at| format!("Resets {}", clock::reset_label(at, now)));
    let here = here.map(|here| here.min(window.percent));
    let elsewhere = window.percent - here.unwrap_or(0);
    let part = |percent: u8| relative(f32::from(percent.min(100)) / 100.);
    let split = here.map(|here| format!("About {here}% in this chat, {elsewhere}% elsewhere"));
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
            h_flex()
                .h(px(6.))
                .w_full()
                .overflow_hidden()
                .rounded_full()
                .bg(theme.muted)
                .child(div().flex_none().h_full().bg(color).w(part(elsewhere)))
                .children(
                    here.map(|here| div().flex_none().h_full().bg(theme.primary).w(part(here))),
                ),
        )
        .children(split.map(|text| {
            div()
                .id("chat-usage-split")
                .test_support()
                .aria_label(gpui_kit::SharedString::from(text.clone()))
                .text_caption()
                .text_color(theme.muted_foreground)
                .child(text)
        }))
        .children(resets.map(|text| {
            div()
                .text_caption()
                .text_color(theme.muted_foreground)
                .child(text)
        }))
}

/// The chat's turns, tokens and changed files, which are what used its
/// part of the plan.
fn this_chat(this_chat: ThisChat, cx: &gpui_kit::App) -> impl IntoElement + use<> {
    let plural = |count: usize, one: &str| {
        if count == 1 {
            format!("1 {one}")
        } else {
            format!("{count} {one}s")
        }
    };
    let mut parts = vec![plural(this_chat.turns, "message")];
    parts.extend(this_chat.tokens.map(|spent| {
        let (fresh, cached) = (tokens(spent.fresh()), tokens(spent.cached));
        format!("{fresh} new tokens, {cached} read from cache")
    }));
    parts.push(format!("{} changed", plural(this_chat.files, "file")));
    v_flex()
        .id("chat-usage-this-chat")
        .test_support()
        .gap_0p5()
        .text_sm()
        .child("This chat")
        .child(
            div()
                .text_caption()
                .text_color(cx.theme().muted_foreground)
                .child(parts.join(" · ")),
        )
}

pub(super) fn level_color(percent: u8, cx: &gpui_kit::App) -> Hsla {
    match percent {
        0..80 => cx.theme().muted_foreground,
        80..100 => cx.theme().warning,
        _ => cx.theme().danger,
    }
}
