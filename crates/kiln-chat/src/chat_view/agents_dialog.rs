//! The subagents view, opened over the chat whose button was pressed:
//! every subagent of the conversation on the left, under the one that
//! started it, running ones first, and on the right the one picked, as a
//! chat of its own: what it was asked, its messages and tool calls, then its
//! report, under its model, context and usage. It follows the chat live.

mod detail;
mod tree;

use super::ChatView;
use crate::{
    scale::{ScaledButton as _, ScaledText as _},
    subagent_view::{self, small_dot},
    transcript::EntryId,
};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, Context, Entity, FocusHandle, KeyDownEvent, SharedString, Subscription,
    TestSupportExt, WeakEntity, Window,
    component::{
        ActiveTheme, IconName,
        button::{Button, ButtonVariants},
        h_flex, v_flex,
    },
    div, px, relative,
};
use tree::Row;

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
    pub fn new(chat: &Entity<ChatView>, shown: Option<EntryId>, cx: &mut Context<Self>) -> Self {
        Self {
            chat: chat.downgrade(),
            focus: cx.focus_handle(),
            selected: shown,
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

    fn show(&mut self, id: EntryId, cx: &mut Context<Self>) {
        self.selected = Some(id);
        cx.notify();
    }

    fn step(&mut self, down: bool, cx: &mut Context<Self>) {
        let Some(chat) = self.chat.upgrade() else {
            return;
        };
        let rows = tree::rows(chat.read(cx).transcript.subagents());
        let next = tree::step(&rows, self.selected, down);
        if let Some(id) = next {
            self.show(id, cx);
        }
    }

    fn on_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "escape" => self.close(window, cx),
            "down" => self.step(true, cx),
            "up" => self.step(false, cx),
            _ => return,
        }
        cx.stop_propagation();
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
                        .row_icon(IconName::Close)
                        .tooltip("Close")
                        .on_click(cx.listener(|this, _, window, cx| this.close(window, cx))),
                ),
            );
        let Some(chat) = self.chat.upgrade() else {
            return body;
        };
        let chat = chat.read(cx);
        let agents = chat.transcript.subagents();
        let listed = tree::rows(agents);
        let selected = self
            .selected
            .and_then(|id| agents.get(id))
            .or_else(|| listed.first().map(|row| row.agent));
        let rows: Vec<AnyElement> = listed
            .iter()
            .map(|row| list_row(*row, selected.map(|agent| agent.id), cx))
            .collect();
        let detail = selected.map(|agent| detail::render(agent, chat, &self.chat, cx));
        body.child(list(rows, listed.len(), cx)).children(detail)
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
                .text_caption()
                .text_color(theme.muted_foreground)
                .child(format!("Subagents · {total}")),
        )
        .children(rows)
}

fn list_row(row: Row, selected: Option<EntryId>, cx: &Context<AgentsDialog>) -> AnyElement {
    let theme = cx.theme();
    let Row { agent, depth } = row;
    let id = agent.id;
    let indents = (0..depth).map(|_| {
        // A line down from each one it is below.
        div()
            .flex_none()
            .self_stretch()
            .w_3()
            .ml_1()
            .border_l_1()
            .border_color(theme.border)
    });
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
            this.show(id, cx);
        }))
        .children(indents)
        .child(small_dot(agent.end, cx))
        .child(
            v_flex()
                .min_w_0()
                .flex_1()
                .child(div().truncate().child(agent.description.clone()))
                .child(
                    div()
                        .text_caption()
                        .truncate()
                        .text_color(theme.muted_foreground)
                        .child(subagent_view::status(agent)),
                ),
        )
        .into_any_element()
}
