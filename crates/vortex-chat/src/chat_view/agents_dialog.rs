//! The subagents view, opened over the chat whose button was pressed:
//! every subagent of the conversation on the left, under the one that
//! started it, running ones first, and on the right the one picked, as a
//! chat of its own: what it was asked, its messages and tool calls, then its
//! report, under its model, context and usage. It follows the chat live.

mod card;
mod detail;
mod map;
mod state;
mod tree;

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Context, Entity, FocusHandle, Global, KeyDownEvent, Pixels, SharedString,
    Subscription, TestSupportExt, WeakEntity, Window,
    assets::IconName as Lucide,
    component::{
        ActiveTheme, IconName,
        button::{Button, ButtonVariants},
        h_flex,
        resizable::{ResizableState, h_resizable, resizable_panel},
        v_flex,
    },
    div, px,
};

use super::ChatView;
use crate::{
    scale::{ScaledButton as _, ScaledText as _},
    subagent_view::{self, small_dot},
    transcript::EntryId,
};
use tree::Row;

const LIST_WIDTH: f32 = 260.;
/// The list's share of a narrow chat, until the user sizes it.
const LIST_SHARE: f32 = 0.36;
const LIST_MIN_WIDTH: f32 = 140.;
const DETAIL_MIN_WIDTH: f32 = 200.;

/// The list's width as the user last sized it, in any chat.
struct ListWidth(Pixels);

impl Global for ListWidth {}

/// Each is a step back from the one after it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    /// The list beside the subagent picked.
    List,
    Map,
    /// Over the map, of this agent or the conversation.
    Card(Option<EntryId>),
    /// The agent's, opened from its card.
    Transcript(EntryId),
}

pub struct AgentsDialog {
    chat: WeakEntity<ChatView>,
    focus: FocusHandle,
    /// The subagent shown; the first in the list until one is picked.
    selected: Option<EntryId>,
    page: Page,
    sections: card::Sections,
    _chat_changed: Subscription,
}

impl AgentsDialog {
    pub fn new(chat: &Entity<ChatView>, shown: Option<EntryId>, cx: &mut Context<Self>) -> Self {
        Self {
            chat: chat.downgrade(),
            focus: cx.focus_handle(),
            selected: shown,
            page: Page::List,
            sections: card::Sections::default(),
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
        if matches!(self.page, Page::Transcript(_)) {
            self.page = Page::Transcript(id);
        }
        cx.notify();
    }

    fn go(&mut self, page: Page, window: &mut Window, cx: &mut Context<Self>) {
        if page != Page::Map && !matches!(page, Page::Transcript(_)) {
            self.sections = card::Sections::default();
        }
        self.page = page;
        // The button pressed is gone with its page, and Escape must not be.
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.page {
            Page::List => self.close(window, cx),
            Page::Map => self.go(Page::List, window, cx),
            Page::Card(_) => self.go(Page::Map, window, cx),
            Page::Transcript(id) => self.go(Page::Card(Some(id)), window, cx),
        }
    }

    fn fold(&mut self, section: &str, open: bool, cx: &mut Context<Self>) {
        if section == "chat-agents-card-prompt" {
            self.sections.prompt = Some(open);
        } else {
            self.sections.calls = open;
        }
        cx.notify();
    }

    fn stop(&self, id: EntryId, cx: &mut Context<Self>) {
        let _ = self.chat.update(cx, |chat, cx| chat.stop_subagent(id, cx));
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
            "escape" => self.back(window, cx),
            "down" if self.page == Page::List => self.step(true, cx),
            "up" if self.page == Page::List => self.step(false, cx),
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
            .text_sm();
        let Some(chat) = self.chat.upgrade() else {
            return body;
        };
        let chat = chat.read(cx);
        let shown = match self.page {
            Page::List => self.list_page(chat, cx),
            Page::Map | Page::Card(_) => map::render(chat, cx),
            Page::Transcript(id) => self.transcript_page(id, chat, cx),
        };
        let card = match self.page {
            Page::Card(id) => card::render(id, chat, self.sections, cx),
            _ => None,
        };
        body.child(shown)
            .when(card.is_none(), |this| this.child(close_button(cx)))
            .children(card)
    }
}

impl AgentsDialog {
    fn list_page(&self, chat: &ChatView, cx: &Context<Self>) -> AnyElement {
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
        let list = list(rows, listed.len(), cx);
        panels(list, detail, chat, cx).into_any_element()
    }

    fn transcript_page(&self, id: EntryId, chat: &ChatView, cx: &Context<Self>) -> AnyElement {
        let agent = chat.transcript.subagents().get(id);
        let back = Button::new("chat-agents-transcript-back")
            .ghost()
            .row_labelled(Lucide::ArrowLeft, "Back")
            .on_click(cx.listener(|this, _, window, cx| this.back(window, cx)));
        v_flex()
            .size_full()
            .child(h_flex().flex_none().p_2().child(back))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .items_start()
                    .children(agent.map(|agent| detail::render(agent, chat, &self.chat, cx))),
            )
            .into_any_element()
    }
}

fn close_button(cx: &Context<AgentsDialog>) -> impl IntoElement + use<> {
    div().absolute().top_2().right_2().child(
        Button::new("chat-agents-close")
            .ghost()
            .row_icon(IconName::Close)
            .tooltip("Close")
            .on_click(cx.listener(|this, _, window, cx| this.close(window, cx))),
    )
}

fn panels(
    list: AnyElement,
    detail: Option<AnyElement>,
    chat: &ChatView,
    cx: &App,
) -> impl IntoElement + use<> {
    let room = chat.transcript_width.get().0;
    let usual = room.map_or(px(LIST_WIDTH), |room| px(LIST_WIDTH).min(room * LIST_SHARE));
    let width = cx.try_global::<ListWidth>().map_or(usual, |kept| kept.0);
    h_resizable("chat-agents-panels")
        .child(
            resizable_panel()
                .size(width.max(px(LIST_MIN_WIDTH)))
                .fixed()
                .size_range(px(LIST_MIN_WIDTH)..Pixels::MAX)
                .child(list),
        )
        .child(
            resizable_panel()
                .size_range(px(DETAIL_MIN_WIDTH)..Pixels::MAX)
                .children(detail),
        )
        .on_resize(
            |state: &Entity<ResizableState>, _: &mut Window, cx: &mut App| {
                if let Some(width) = state.read(cx).sizes().first().copied() {
                    cx.set_global(ListWidth(width));
                }
            },
        )
}

fn list(rows: Vec<AnyElement>, total: usize, cx: &Context<AgentsDialog>) -> AnyElement {
    let theme = cx.theme();
    v_flex()
        .id("chat-agents-list")
        .test_support()
        .size_full()
        .overflow_y_scroll()
        .p_2()
        .gap_1()
        .child(
            h_flex()
                .gap_1()
                .pl_1p5()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_caption()
                        .text_color(theme.muted_foreground)
                        .child(format!("Subagents · {total}")),
                )
                .child(
                    Button::new("chat-agents-map-show")
                        .ghost()
                        .row_icon(Lucide::Network)
                        .tooltip("Show the Agent Map")
                        .on_click(
                            cx.listener(|this, _, window, cx| this.go(Page::Map, window, cx)),
                        ),
                ),
        )
        .children(rows)
        .into_any_element()
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
