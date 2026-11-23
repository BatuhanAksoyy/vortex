//! A chat as a tab in the dock. Each one runs its own agent process, which
//! keeps working while another tab is shown.

use gpui_kit::prelude::*;
use gpui_kit::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, SharedString, Subscription, Window,
    component::{
        ActiveTheme, Icon, Sizable as _,
        button::{Button, ButtonVariants as _},
        dock::{BasePanel, Panel, PanelEvent},
        h_flex,
    },
    div,
};
use kiln_chat::{ChatView, Status};

/// Tab titles past this many characters are cut, so a few tabs fit.
const TITLE_CHARS: usize = 28;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatPanelEvent {
    /// The tab was shown or focused; the sidebar follows it.
    Activated,
    /// The tab's "New session" button: another chat in the same group.
    NewTab,
    /// The tab's "Split right" button: another chat in a group beside it.
    SplitRight,
    /// The tab was closed; its process ends with it.
    Removed,
}

pub struct ChatPanel {
    chat: Entity<ChatView>,
    /// The dock tracks the active panel's handle on the group around it, so
    /// it must not be the chat's own, which the composer tracks too: one
    /// focus reported by two elements panics GPUI's accessibility tree.
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl ChatPanel {
    pub fn new(chat: Entity<ChatView>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        let subscriptions = vec![
            // The tab title and its running mark come from the chat.
            cx.observe(&chat, |_, _, cx| cx.notify()),
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
            focus,
            _subscriptions: subscriptions,
        }
    }

    pub fn chat(&self) -> &Entity<ChatView> {
        &self.chat
    }
}

impl EventEmitter<PanelEvent> for ChatPanel {}
impl EventEmitter<ChatPanelEvent> for ChatPanel {}

impl BasePanel for ChatPanel {
    fn panel_name(&self) -> &'static str {
        "ChatPanel"
    }

    fn set_active(&mut self, active: bool, _: &mut Window, cx: &mut Context<Self>) {
        if active {
            cx.emit(ChatPanelEvent::Activated);
        }
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
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.chat.clone())
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
