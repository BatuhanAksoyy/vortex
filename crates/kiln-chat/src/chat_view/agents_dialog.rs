//! The subagents view, opened over the chat whose button was pressed:
//! every subagent of the conversation on the left,
//! running ones first, and on the right the one picked, as a chat of its
//! own: what it was asked, its messages and tool calls, then its report,
//! under its model, context and usage. It follows the chat live.

use std::path::Path;

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Context, Entity, FocusHandle, FontWeight, KeyDownEvent, SharedString,
    Subscription, TestSupportExt, WeakEntity, Window,
    component::{
        ActiveTheme, IconName, Sizable,
        button::{Button, ButtonVariants},
        h_flex,
        text::TextView,
        v_flex,
    },
    div, px, relative,
};
use kiln_agent::SubagentEnd;

use super::ChatView;
use crate::{
    agent_state::tokens,
    literal::literal_markdown,
    subagent_view,
    transcript::{Card, EntryId},
};

const LIST_MAX_WIDTH: f32 = 260.;
/// The list's share of the dialog's width, below its widest.
const LIST_SHARE: f32 = 0.36;

pub struct AgentsDialog {
    chat: WeakEntity<ChatView>,
    focus: FocusHandle,
    /// The subagent shown; the first in the list until one is picked.
    selected: Option<EntryId>,
    _chat_changed: Subscription,
}

impl AgentsDialog {
    pub fn new(chat: &Entity<ChatView>, cx: &mut Context<Self>) -> Self {
        Self {
            chat: chat.downgrade(),
            focus: cx.focus_handle(),
            selected: None,
            _chat_changed: cx.observe(chat, |_, _, cx| cx.notify()),
        }
    }

    pub fn focus_handle(&self) -> FocusHandle {
        self.focus.clone()
    }

    fn close(&self, window: &mut Window, cx: &mut Context<Self>) {
        let _ = self
            .chat
            .update(cx, |chat, cx| chat.close_agents(window, cx));
    }

    fn show_in_chat(&self, id: EntryId, window: &mut Window, cx: &mut Context<Self>) {
        self.close(window, cx);
        let _ = self
            .chat
            .update(cx, |chat, cx| chat.reveal_subagent(id, cx));
    }

    fn on_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key == "escape" {
            self.close(window, cx);
            cx.stop_propagation();
        }
    }
}

impl Render for AgentsDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let body = h_flex()
            .id("chat-agents-dialog")
            .test_support()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .relative()
            .size_full()
            .items_start()
            .rounded(theme.radius_lg)
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .shadow_lg()
            .text_sm()
            .child(
                div().absolute().top_2().right_2().child(
                    Button::new("chat-agents-close")
                        .ghost()
                        .xsmall()
                        .icon(IconName::Close)
                        .tooltip("Close")
                        .on_click(cx.listener(|this, _, window, cx| this.close(window, cx))),
                ),
            );
        let Some(chat) = self.chat.upgrade() else {
            return body;
        };
        let chat = chat.read(cx);
        let mut cards: Vec<Card> = chat.transcript.subagents().collect();
        cards.reverse();
        cards.sort_by_key(|card| card.agent.end.is_some());
        let selected = self
            .selected
            .and_then(|id| cards.iter().find(|card| card.id == id))
            .or_else(|| cards.first())
            .copied();
        let rows: Vec<AnyElement> = cards
            .iter()
            .map(|card| list_row(card, selected.map(|card| card.id), cx))
            .collect();
        let detail = selected.map(|card| detail(card, &chat.cwd, cx));
        body.child(list(rows, cards.len(), cx)).children(detail)
    }
}

fn list(rows: Vec<AnyElement>, total: usize, cx: &Context<AgentsDialog>) -> impl IntoElement {
    let theme = cx.theme();
    v_flex()
        .id("chat-agents-list")
        .flex_none()
        .w(relative(LIST_SHARE))
        .max_w(px(LIST_MAX_WIDTH))
        .h_full()
        .overflow_y_scroll()
        .p_2()
        .gap_1()
        .border_r_1()
        .border_color(theme.border)
        .child(
            div()
                .px_1p5()
                .pb_1()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(format!("Subagents · {total}")),
        )
        .children(rows)
}

fn list_row(card: &Card, selected: Option<EntryId>, cx: &Context<AgentsDialog>) -> AnyElement {
    let theme = cx.theme();
    let id = card.id;
    let agent = card.agent;
    h_flex()
        .id(SharedString::from(format!("chat-agent-{id}")))
        .test_support()
        .aria_label(SharedString::from(agent.description.clone()))
        .items_start()
        .gap_2()
        .px_1p5()
        .py_1()
        .rounded(theme.radius)
        .cursor_pointer()
        .when(selected == Some(id), |this| this.bg(theme.secondary))
        .hover(|style| style.bg(theme.secondary))
        .on_click(cx.listener(move |this, _, _, cx| {
            this.selected = Some(id);
            cx.notify();
        }))
        .child(dot(agent.end, cx))
        .child(
            v_flex()
                .min_w_0()
                .flex_1()
                .child(div().truncate().child(agent.description.clone()))
                .child(
                    div()
                        .text_xs()
                        .truncate()
                        .text_color(theme.muted_foreground)
                        .child(subagent_view::status(agent)),
                ),
        )
        .into_any_element()
}

fn dot(end: Option<SubagentEnd>, cx: &App) -> impl IntoElement {
    div()
        .flex_none()
        .mt(px(6.))
        .size(px(7.))
        .rounded_full()
        .bg(subagent_view::dot(end, cx))
}

/// The picked subagent: its facts, then its chat.
fn detail(card: Card, root: &Path, cx: &Context<AgentsDialog>) -> AnyElement {
    let agent = card.agent;
    let id = card.id;
    let header = h_flex()
        .gap_2()
        .child(dot(agent.end, cx))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .font_weight(FontWeight::SEMIBOLD)
                .child(agent.description.clone()),
        )
        .child(
            Button::new("chat-agent-reveal")
                .ghost()
                .xsmall()
                .label("Show in chat")
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.show_in_chat(id, window, cx);
                })),
        );
    v_flex()
        .id("chat-agent-detail")
        .test_support()
        .aria_label(SharedString::from(agent.description.clone()))
        .flex_1()
        .min_w_0()
        .h_full()
        .overflow_y_scroll()
        .p_3()
        .gap_3()
        .child(header.pr_6())
        .child(facts(card, cx))
        .child(div().h_px().bg(cx.theme().border))
        .children(prompt(card, cx))
        .children(agent.steps.iter().enumerate().map(|(index, step)| {
            let step_id = SharedString::from(format!("chat-agent-step-{id}-{index}"));
            subagent_view::step_row(step, step_id, root, cx)
        }))
        .children(report(card, cx))
        .into_any_element()
}

fn facts(card: Card, cx: &App) -> impl IntoElement {
    let agent = card.agent;
    let context = agent.request.map_or_else(
        || "Not reported yet".to_owned(),
        |request| {
            let total = request.cached + request.written + request.uncached;
            format!(
                "{} tokens in the last request · {} read from cache",
                tokens(total),
                tokens(request.cached)
            )
        },
    );
    let fact = |name: &'static str, value: String| {
        h_flex()
            .id(SharedString::from(format!(
                "chat-agent-{}",
                name.to_lowercase()
            )))
            .test_support()
            .aria_label(SharedString::from(value.clone()))
            .gap_2()
            .child(
                div()
                    .flex_none()
                    .w(px(72.))
                    .text_color(cx.theme().muted_foreground)
                    .child(name),
            )
            .child(div().min_w_0().child(value))
    };
    let unknown = || "Not reported yet".to_owned();
    v_flex()
        .gap_1()
        .child(fact("Status", subagent_view::status(agent)))
        .child(fact("Type", agent.kind.clone().unwrap_or_else(unknown)))
        .child(fact("Model", agent.model.clone().unwrap_or_else(unknown)))
        .child(fact("Context", context))
}

/// What the main agent asked it, as the chat's first message.
fn prompt(card: Card, cx: &App) -> Option<impl IntoElement> {
    let text = card.call.input.get("prompt")?.as_str()?;
    let theme = cx.theme();
    Some(
        div()
            .px_3()
            .py_1p5()
            .rounded(theme.radius * 1.5)
            .border_1()
            .border_color(theme.border)
            .bg(theme.secondary)
            .child(
                TextView::markdown(("chat-agent-prompt", card.id), literal_markdown(text))
                    .selectable(true),
            ),
    )
}

fn report(card: Card, cx: &App) -> Option<impl IntoElement> {
    let summary = card.agent.summary.clone()?;
    let failed = card.agent.end != Some(SubagentEnd::Completed);
    let theme = cx.theme();
    Some(
        v_flex()
            .gap_1()
            .pt_1()
            .border_t_1()
            .border_color(theme.border)
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(if failed { "Why it ended" } else { "Report" }),
            )
            .child(TextView::markdown(("chat-agent-report", card.id), summary).selectable(true)),
    )
}
