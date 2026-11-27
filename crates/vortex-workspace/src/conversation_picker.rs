//! The conversations page a chat opens from its name, over the chat like
//! its subagents and processes: the sidebar's conversations, searchable by
//! name, under the same day headings. A click switches to one.

use std::time::SystemTime;

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Context, Entity, EventEmitter, FocusHandle, FontWeight, KeyDownEvent,
    SharedString, Subscription, TestSupportExt, Window,
    component::{
        ActiveTheme, IconName, Sizable as _,
        button::{Button, ButtonVariants as _},
        h_flex,
        input::{Input, InputEvent, InputState},
        v_flex,
    },
    div, rems,
};
use vortex_agent::PastSession;
use vortex_chat::scale::{ScaledButton as _, ScaledText as _};

use crate::session_list::{SessionList, day_group, heading, stamp};

const SEARCH_REMS: f32 = 16.0;
const NONE_MATCH: &str = "No conversations match the search.";

pub enum ConversationPickerEvent {
    Pick { id: String, open_elsewhere: bool },
    Close,
}

pub struct ConversationPicker {
    sessions: Entity<SessionList>,
    /// The conversation the chat shows, which is marked, not opened.
    current: Option<String>,
    search: Entity<InputState>,
    focus: FocusHandle,
    _subscriptions: [Subscription; 2],
}

impl EventEmitter<ConversationPickerEvent> for ConversationPicker {}

impl ConversationPicker {
    pub fn new(
        sessions: Entity<SessionList>,
        current: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search conversations"));
        let subscriptions = [
            cx.observe(&sessions, |_, _, cx| cx.notify()),
            cx.subscribe(&search, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
        ];
        Self {
            sessions,
            current,
            search,
            focus: cx.focus_handle(),
            _subscriptions: subscriptions,
        }
    }

    /// Focuses the page, not its search, as the processes page does, so
    /// Escape works at once.
    pub fn focus(page: &Entity<Self>, window: &mut Window, cx: &mut App) {
        let focus = page.read(cx).focus.clone();
        window.focus(&focus, cx);
    }

    fn on_key(event: &KeyDownEvent, cx: &mut Context<Self>) {
        if event.keystroke.key == "escape" {
            cx.emit(ConversationPickerEvent::Close);
            cx.stop_propagation();
        }
    }

    fn pick(&mut self, session: &PastSession, cx: &mut Context<Self>) {
        if self.current.as_deref() == Some(session.id.as_str()) {
            cx.emit(ConversationPickerEvent::Close);
        } else {
            cx.emit(ConversationPickerEvent::Pick {
                id: session.id.clone(),
                open_elsewhere: session.open_elsewhere,
            });
        }
    }

    fn top_bar(&self, cx: &Context<Self>) -> impl IntoElement {
        h_flex()
            .gap_3()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .flex_1()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Conversations"),
            )
            .child(
                div()
                    .id("conversations-search")
                    .test_support()
                    .w(rems(SEARCH_REMS))
                    .child(Input::new(&self.search).small().cleanable(true)),
            )
            .child(
                Button::new("conversations-close")
                    .ghost()
                    .row_icon(IconName::Close)
                    .tooltip("Close")
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(ConversationPickerEvent::Close))),
            )
    }

    fn row(&self, session: &PastSession, now: SystemTime, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let current = self.current.as_deref() == Some(session.id.as_str());
        let running = self.sessions.read(cx).is_running(&session.id);
        let hover = theme.secondary.opacity(0.5);
        let picked = session.clone();
        h_flex()
            .id(SharedString::from(format!("conversation-{}", session.id)))
            .test_support()
            .aria_label(SharedString::from(session.title.clone()))
            .gap_2()
            .px_2()
            .py_1p5()
            .rounded(theme.radius)
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .when(current, |this| {
                this.bg(theme.secondary).font_weight(FontWeight::MEDIUM)
            })
            .on_click(cx.listener(move |this, _, _, cx| this.pick(&picked, cx)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(session.title.clone()),
            )
            .child(stamp(session, running, now, cx))
            .into_any_element()
    }

    /// The conversations matching the search, under their day headings.
    fn rows(&self, cx: &Context<Self>) -> Vec<AnyElement> {
        let query = self.search.read(cx).value().trim().to_lowercase();
        let now = SystemTime::now();
        let today = jiff::Zoned::now().date();
        let mut shown = Vec::new();
        let mut last_group = None;
        let sessions = self.sessions.read(cx).sessions();
        let matching = sessions
            .iter()
            .filter(|session| query.is_empty() || session.title.to_lowercase().contains(&query));
        for session in matching {
            let group = day_group(session.updated_at, today);
            if last_group != Some(group) {
                last_group = Some(group);
                shown.push(heading(group, cx).into_any_element());
            }
            shown.push(self.row(session, now, cx));
        }
        shown
    }

    /// What shows in place of rows: the list's own note, or that the
    /// search matches none.
    fn note(&self, none_shown: bool, cx: &App) -> Option<String> {
        let list = self.sessions.read(cx);
        list.note()
            .or_else(|| (none_shown && !list.sessions().is_empty()).then(|| NONE_MATCH.to_owned()))
    }
}

impl Render for ConversationPicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let rows = self.rows(cx);
        let note = self.note(rows.is_empty(), cx).map(|note| {
            div()
                .id("conversations-note")
                .test_support()
                .aria_label(SharedString::from(note.clone()))
                .px_2()
                .pt_3()
                .text_caption()
                .text_color(theme.muted_foreground)
                .child(note)
        });
        v_flex()
            .id("conversations")
            .test_support()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|_, event, _, cx| Self::on_key(event, cx)))
            .size_full()
            .rounded(theme.radius_lg)
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .shadow_lg()
            .text_sm()
            .child(self.top_bar(cx))
            .child(
                v_flex()
                    .id("conversations-rows")
                    .flex_1()
                    .min_h_0()
                    .p_2()
                    .gap_0p5()
                    .overflow_y_scroll()
                    .children(note)
                    .children(rows),
            )
    }
}
