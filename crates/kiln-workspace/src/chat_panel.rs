//! A chat as a tab in the dock. Each one runs its own agent process, which
//! keeps working while another tab is shown.

use gpui_kit::prelude::*;
use gpui_kit::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, SharedString, Subscription,
    TestSupportExt as _, Window,
    component::{
        ActiveTheme, Icon, Sizable as _,
        button::{Button, ButtonVariants as _},
        dock::{BasePanel, Panel, PanelEvent},
        h_flex,
    },
    div,
};
use kiln_chat::{ChatEvent, ChatView, Status};

use crate::{
    conversation_picker::{ConversationPicker, ConversationPickerEvent},
    session_list::SessionList,
};

/// Tab titles past this many characters are cut, so a few tabs fit.
const TITLE_CHARS: usize = 28;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatPanelEvent {
    /// The user clicked in the tab or focused it: a new chat goes beside
    /// it.
    Activated,
    /// The tab's "New session" button: another chat in the same group.
    NewTab,
    /// The tab's "Split right" button: another chat in a group beside it.
    SplitRight,
    /// The tab was closed; its process ends with it.
    Removed,
    /// A conversation picked on the chat's conversations page, to show in
    /// this chat.
    Open { id: String, open_elsewhere: bool },
}

pub struct ChatPanel {
    chat: Entity<ChatView>,
    sessions: Entity<SessionList>,
    /// The conversations page over the chat, while open.
    picker: Option<(Entity<ConversationPicker>, Subscription)>,
    /// The dock tracks the active panel's handle on the group around it, so
    /// it must not be the chat's own, which the composer tracks too: one
    /// focus reported by two elements panics GPUI's accessibility tree.
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl ChatPanel {
    pub fn new(
        chat: Entity<ChatView>,
        sessions: Entity<SessionList>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        let subscriptions = vec![
            // The tab title and its running mark come from the chat.
            cx.observe(&chat, |_, _, cx| cx.notify()),
            cx.subscribe_in(&chat, window, |this, _, event: &ChatEvent, window, cx| {
                if matches!(event, ChatEvent::PickConversation) {
                    this.open_picker(window, cx);
                }
            }),
            cx.on_focus_in(&focus, window, |_, _, cx| {
                cx.emit(ChatPanelEvent::Activated);
            }),
            // The dock focuses the panel when its tab is chosen.
            cx.on_focus(&focus, window, |this, window, cx| {
                this.chat.read(cx).focus_handle(cx).focus(window, cx);
            }),
        ];
        Self {
            chat,
            sessions,
            picker: None,
            focus,
            _subscriptions: subscriptions,
        }
    }

    pub fn chat(&self) -> &Entity<ChatView> {
        &self.chat
    }

    fn open_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.chat.read(cx).session_id().map(str::to_owned);
        let sessions = self.sessions.clone();
        let picker = cx.new(|cx| ConversationPicker::new(sessions, current, window, cx));
        let subscription = cx.subscribe_in(&picker, window, |this, _, event, window, cx| {
            this.close_picker(window, cx);
            if let ConversationPickerEvent::Pick { id, open_elsewhere } = event {
                cx.emit(ChatPanelEvent::Open {
                    id: id.clone(),
                    open_elsewhere: *open_elsewhere,
                });
            }
        });
        ConversationPicker::focus(&picker, window, cx);
        self.picker = Some((picker, subscription));
        cx.notify();
    }

    fn close_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.picker.take().is_some() {
            self.chat.read(cx).focus_handle(cx).focus(window, cx);
            cx.notify();
        }
    }
}

impl EventEmitter<PanelEvent> for ChatPanel {}
impl EventEmitter<ChatPanelEvent> for ChatPanel {}

impl BasePanel for ChatPanel {
    fn panel_name(&self) -> &'static str {
        "ChatPanel"
    }

    fn on_removed(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(ChatPanelEvent::Removed);
    }
}

impl Panel for ChatPanel {
    fn title(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let chat = self.chat.read(cx);
        let running = chat.status() == Status::Running;
        h_flex()
            .gap_1p5()
            .when(running, |this| {
                this.child(div().size_1p5().rounded_full().bg(cx.theme().primary))
            })
            .child(tab_title(chat.title()))
    }

    fn tab_name(&self, cx: &App) -> Option<SharedString> {
        Some(tab_title(self.chat.read(cx).title()))
    }

    /// Drawn here rather than as toolbar buttons, which the dock shrinks to
    /// a size where the icons blur on a standard-density display.
    fn title_suffix(&mut self, _: &mut Window, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        Some(
            h_flex()
                .gap_1()
                .child(
                    Button::new("chat-new-tab")
                        .ghost()
                        .small()
                        .tab_stop(false)
                        .icon(Icon::new(gpui_kit::assets::IconName::Plus))
                        .tooltip("New session")
                        .on_click(cx.listener(|_, _, _, cx| cx.emit(ChatPanelEvent::NewTab))),
                )
                .child(
                    Button::new("chat-split-right")
                        .ghost()
                        .small()
                        .tab_stop(false)
                        .icon(Icon::new(gpui_kit::assets::IconName::Columns2))
                        .tooltip("Split right")
                        .on_click(cx.listener(|_, _, _, cx| cx.emit(ChatPanelEvent::SplitRight))),
                ),
        )
    }

    /// The chat draws its own margins.
    fn inner_padding(&self, _: &App) -> bool {
        false
    }
}

fn tab_title(title: Option<&str>) -> SharedString {
    let Some(title) = title else {
        return "New session".into();
    };
    if title.chars().count() <= TITLE_CHARS {
        return title.to_owned().into();
    }
    let cut: String = title.chars().take(TITLE_CHARS - 1).collect();
    format!("{}…", cut.trim_end()).into()
}

impl Focusable for ChatPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for ChatPanel {
    /// A click anywhere in the chat makes it the one a new chat goes
    /// beside, also on text or buttons that take no focus. The dock also
    /// shows each group's tab when it lays them out, which says nothing of
    /// where the user works, so being shown does not.
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The backdrop keeps clicks from reaching the chat under it.
        let picker = self.picker.as_ref().map(|(picker, _)| {
            div()
                .id("conversations-overlay")
                .test_support()
                .absolute()
                .inset_0()
                .p_3()
                .occlude()
                .bg(cx.theme().overlay)
                .child(picker.clone())
        });
        div()
            .relative()
            .size_full()
            .capture_any_mouse_down(cx.listener(|_, _, _, cx| cx.emit(ChatPanelEvent::Activated)))
            .child(self.chat.clone())
            .children(picker)
    }
}

#[cfg(test)]
mod tests {
    use super::tab_title;

    #[test]
    fn long_titles_are_cut_to_fit_a_tab() {
        assert_eq!(tab_title(None), "New session");
        assert_eq!(tab_title(Some("Fix the build")), "Fix the build");
        let long = tab_title(Some("Replace the file tree with the session history"));
        assert!(long.chars().count() <= 28, "{long}");
        assert!(long.ends_with('…'));
    }
}
