//! The chats in the dock: where a new or reopened conversation goes, and
//! which chat the sidebar follows.

use std::collections::HashSet;

use gpui_kit::{
    AppContext as _, Context, Entity, Focusable as _, Subscription, Window,
    component::dock::{
        DockArea, DockLayout, DockPlacement, DockSkin, InsertTarget, NodeId, PanelId, PanelStyle,
        panel_handle,
    },
};
use kiln_chat::{ChatEvent, ChatView, Status};

use super::Workspace;
use crate::{
    chat_panel::{ChatPanel, ChatPanelEvent},
    saved_layout::SavedChat,
    session_list::{SessionList, SessionListEvent},
};

pub(super) struct ChatTab {
    panel: Entity<ChatPanel>,
    _subscriptions: [Subscription; 2],
}

/// Where a new chat goes, relative to the focused one.
#[derive(Clone, Copy)]
pub(super) enum Place {
    Tab,
    Right,
}

pub(super) fn new_dock(window: &mut Window, cx: &mut gpui_kit::App) -> Entity<DockArea> {
    let (area, skin) = DockSkin::dock_area("chats", None, window, cx);
    skin.set_panel_style(PanelStyle::TabBar, cx);
    skin.set_close_button_visible(true, cx);
    area
}

impl Workspace {
    /// A chat for the project, not yet placed in the dock.
    pub(super) fn new_chat_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<ChatPanel> {
        let (backend, accounts, root) = (
            self.backend.clone(),
            self.accounts.clone(),
            self.root.clone(),
        );
        let chat = cx.new(|cx| ChatView::new(backend, accounts, root, window, cx));
        let panel = cx.new(|cx| ChatPanel::new(chat.clone(), window, cx));
        let subscriptions = [
            cx.subscribe_in(&panel, window, |this, panel, event, window, cx| {
                this.on_chat_panel(panel, *event, window, cx);
            }),
            cx.subscribe(&chat, |this, _, event: &ChatEvent, cx| {
                this.on_chat(*event, cx);
            }),
        ];
        self.chats.push(ChatTab {
            panel: panel.clone(),
            _subscriptions: subscriptions,
        });
        panel
    }

    /// Starts a chat for the project beside the focused one.
    pub(super) fn add_chat(
        &mut self,
        place: Place,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<ChatView> {
        let panel = self.new_chat_panel(window, cx);
        let chat = panel.read(cx).chat().clone();
        let beside = self.focused_group(cx);
        let id = PanelId::from(panel.entity_id());
        self.dock.update(cx, |dock, cx| {
            dock.add_panel_view(
                panel_handle(panel.clone()),
                DockPlacement::Center,
                None,
                window,
                cx,
            );
            let Some(node) = beside else {
                return;
            };
            // Moved, not `split_at`, which adds the panel beside `node` but
            // leaves it in the group it was added to as well.
            let target = match place {
                Place::Tab => InsertTarget::Tabs {
                    node,
                    ix: None,
                    activate: true,
                },
                Place::Right => InsertTarget::Split {
                    node,
                    placement: gpui_kit::base::Placement::Right,
                    size: None,
                },
            };
            dock.move_panel(id, target, window, cx);
        });
        self.active_chat = Some(panel);
        self.sync_sidebar(cx);
        chat
    }

    /// Ends every chat, leaving the dock empty.
    pub(super) fn clear_chats(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.chats.clear();
        self.active_chat = None;
        self.dock.update(cx, |dock, cx| {
            dock.set_center(DockLayout::h_split(), window, cx);
        });
    }

    /// The tab group holding the chat the user last worked in.
    fn focused_group(&self, cx: &gpui_kit::App) -> Option<NodeId> {
        let id = PanelId::from(self.active_chat.as_ref()?.entity_id());
        self.dock
            .read(cx)
            .layout(DockPlacement::Center)?
            .find_panel_node(id)
    }

    /// The focused chat while nothing was said in it, otherwise a new tab.
    fn blank_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<ChatView> {
        if let Some(panel) = &self.active_chat {
            let chat = panel.read(cx).chat().clone();
            if chat.read(cx).is_blank() {
                return chat;
            }
        }
        self.add_chat(Place::Tab, window, cx)
    }

    /// The conversation the chat in dock panel `id` shows, and its account.
    pub(super) fn saved_chat_in(&self, id: PanelId, cx: &gpui_kit::App) -> Option<SavedChat> {
        let tab = self
            .chats
            .iter()
            .find(|tab| PanelId::from(tab.panel.entity_id()) == id)?;
        let chat = tab.panel.read(cx).chat().read(cx);
        Some(SavedChat {
            session: chat.session_id()?.to_owned(),
            account: chat.account().map(str::to_owned),
        })
    }

    fn chat_showing(&self, session_id: &str, cx: &gpui_kit::App) -> Option<Entity<ChatPanel>> {
        self.chats
            .iter()
            .map(|tab| &tab.panel)
            .find(|panel| panel.read(cx).chat().read(cx).session_id() == Some(session_id))
            .cloned()
    }

    pub(super) fn on_session_list(
        &mut self,
        _: &Entity<SessionList>,
        event: &SessionListEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (id, open_elsewhere) = match event {
            SessionListEvent::New => {
                let chat = self.blank_chat(window, cx);
                focus(&chat, window, cx);
                return;
            }
            SessionListEvent::Loaded => {
                self.restore_layout(window, cx);
                return;
            }
            SessionListEvent::Open { id, open_elsewhere } => (id, *open_elsewhere),
        };
        if let Some(panel) = self.chat_showing(id, cx) {
            let panel_id = PanelId::from(panel.entity_id());
            self.dock
                .update(cx, |dock, cx| dock.select_panel(panel_id, window, cx));
            let chat = panel.read(cx).chat().clone();
            focus(&chat, window, cx);
            return;
        }
        let chat = self.blank_chat(window, cx);
        chat.update(cx, |chat, cx| {
            chat.open_session(id, open_elsewhere, window, cx);
        });
    }

    fn on_chat_panel(
        &mut self,
        panel: &Entity<ChatPanel>,
        event: ChatPanelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let place = match event {
            ChatPanelEvent::Activated => {
                self.active_chat = Some(panel.clone());
                self.sync_sidebar(cx);
                return;
            }
            ChatPanelEvent::Removed => {
                self.remove_chat(panel, cx);
                return;
            }
            ChatPanelEvent::NewTab => Place::Tab,
            ChatPanelEvent::SplitRight => Place::Right,
        };
        self.active_chat = Some(panel.clone());
        let chat = self.add_chat(place, window, cx);
        focus(&chat, window, cx);
    }

    /// Forgets a closed tab, which ends its process. The dock never closes
    /// the last one.
    fn remove_chat(&mut self, panel: &Entity<ChatPanel>, cx: &mut Context<Self>) {
        let Some(ix) = self.chats.iter().position(|tab| &tab.panel == panel) else {
            return;
        };
        self.chats.remove(ix);
        if self.active_chat.as_ref() == Some(panel) {
            self.active_chat = self.chats.last().map(|tab| tab.panel.clone());
        }
        self.sync_sidebar(cx);
    }

    fn on_chat(&mut self, event: ChatEvent, cx: &mut Context<Self>) {
        self.sync_sidebar(cx);
        match event {
            ChatEvent::SessionChanged => {
                self.reload_sessions(cx);
                self.save_layout(cx);
            }
            ChatEvent::AccountChanged => self.save_layout(cx),
            ChatEvent::TurnStarted => {}
        }
    }

    pub(super) fn sync_sidebar(&mut self, cx: &mut Context<Self>) {
        let current = self.active_chat.as_ref().and_then(|panel| {
            panel
                .read(cx)
                .chat()
                .read(cx)
                .session_id()
                .map(str::to_owned)
        });
        let running: HashSet<String> = self
            .chats
            .iter()
            .map(|tab| tab.panel.read(cx).chat().read(cx))
            .filter(|chat| chat.status() == Status::Running)
            .filter_map(|chat| chat.session_id().map(str::to_owned))
            .collect();
        self.sessions.update(cx, |sessions, cx| {
            sessions.set_current(current, running, cx);
        });
    }
}

fn focus(chat: &Entity<ChatView>, window: &mut Window, cx: &mut gpui_kit::App) {
    chat.read(cx).focus_handle(cx).focus(window, cx);
}
