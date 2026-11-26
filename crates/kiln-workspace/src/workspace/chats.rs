//! The chats in the dock: where a new chat goes, and switching a chat to
//! another conversation.

use std::collections::HashSet;

use gpui_kit::{
    AppContext as _, Context, Entity, Focusable as _, SharedString, Subscription, WeakEntity,
    Window,
    component::dock::{
        DockArea, DockPlacement, DockSkin, InsertTarget, NodeId, PaneNode, PaneRef, PanelId,
        PanelStyle, panel_handle,
    },
};
use kiln_chat::{ChatEvent, ChatView, Status};

use super::Workspace;
use crate::{
    chat_panel::{ChatPanel, ChatPanelEvent},
    project::Project,
    saved_layout::SavedChat,
};

pub(super) struct ChatTab {
    pub(super) panel: Entity<ChatPanel>,
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
    /// A chat in `project`, not yet placed in the dock.
    pub(super) fn new_chat_panel(
        &mut self,
        project: &Entity<Project>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<ChatPanel> {
        let (backend, accounts, root) = (
            self.backend.clone(),
            self.accounts.clone(),
            project.read(cx).root().to_owned(),
        );
        let notes = self.data_dir.as_ref().map(|dir| dir.join("notes"));
        let chat = cx.new(|cx| {
            let mut chat = ChatView::new(backend, accounts, root, window, cx);
            if let Some(dir) = notes {
                chat.keep_notes_in(dir);
            }
            chat
        });
        let panel = cx.new(|cx| ChatPanel::new(chat.clone(), project.clone(), window, cx));
        self.give_tools(&panel, cx);
        let weak = panel.downgrade();
        let subscriptions = [
            cx.subscribe_in(&panel, window, |this, panel, event, window, cx| {
                this.on_chat_panel(panel, event, window, cx);
            }),
            cx.subscribe_in(&chat, window, move |this, _, event, window, cx| {
                this.on_chat(&weak, event, window, cx);
            }),
        ];
        self.chats.push(ChatTab {
            panel: panel.clone(),
            _subscriptions: subscriptions,
        });
        self.sync_alone(cx);
        panel
    }

    /// Starts a chat in `project` beside the focused one.
    pub(super) fn add_chat(
        &mut self,
        place: Place,
        project: &Entity<Project>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<ChatView> {
        // A new tab goes on in the mode the user last worked in.
        let mode = (self.active_chat.as_ref())
            .and_then(|active| active.read(cx).chat().read(cx).known_mode());
        let panel = self.new_chat_panel(project, window, cx);
        let chat = panel.read(cx).chat().clone();
        if let Some(mode) = mode {
            chat.update(cx, |chat, cx| chat.start_in_mode(mode, cx));
        }
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
        self.activate(Some(panel), window, cx);
        self.sync_running(cx);
        chat
    }

    /// The tab group holding the chat the user last worked in.
    fn focused_group(&self, cx: &gpui_kit::App) -> Option<NodeId> {
        let id = PanelId::from(self.active_chat.as_ref()?.entity_id());
        self.dock
            .read(cx)
            .layout(DockPlacement::Center)?
            .find_panel_node(id)
    }

    /// The conversation the chat in dock panel `id` shows, and its account.
    pub(super) fn saved_chat_in(&self, id: PanelId, cx: &gpui_kit::App) -> Option<SavedChat> {
        let tab = self
            .chats
            .iter()
            .find(|tab| PanelId::from(tab.panel.entity_id()) == id)?;
        let chat = tab.panel.read(cx).chat().read(cx);
        Some(SavedChat {
            folder: Some(chat.cwd().to_owned()),
            session: chat.session_id().map(str::to_owned),
            account: chat.account().map(str::to_owned),
            mode: chat.known_mode(),
            effort: chat.known_effort(),
        })
    }

    /// Each running agent's process, under its chat's name.
    pub(super) fn agent_roots(&self, cx: &gpui_kit::App) -> Vec<(SharedString, u32)> {
        self.chats
            .iter()
            .filter_map(|tab| {
                let chat = tab.panel.read(cx).chat().read(cx);
                let title = chat.title().unwrap_or("New session");
                Some((SharedString::from(title.to_owned()), chat.process_id()?))
            })
            .collect()
    }

    fn chat_showing(&self, session_id: &str, cx: &gpui_kit::App) -> Option<Entity<ChatPanel>> {
        self.chats
            .iter()
            .map(|tab| &tab.panel)
            .find(|panel| panel.read(cx).chat().read(cx).session_id() == Some(session_id))
            .cloned()
    }

    /// Names each chat after its conversation in the stored list, as the
    /// conversations page shows it.
    pub(super) fn sync_titles(&self, project: &Entity<Project>, cx: &mut Context<Self>) {
        let sessions = project.read(cx).sessions().read(cx).sessions().to_vec();
        for tab in &self.chats {
            if tab.panel.read(cx).project() != project {
                continue;
            }
            let chat = tab.panel.read(cx).chat().clone();
            chat.update(cx, |chat, cx| {
                let listed = chat
                    .session_id()
                    .and_then(|id| sessions.iter().find(|session| session.id == id));
                chat.set_stored_title(listed.map(|session| session.title.as_str()), cx);
            });
        }
    }

    /// Switches the active chat to the conversation `id`, or shows the tab
    /// that shows it already, so no two processes run one conversation. A
    /// chat in the middle of a turn keeps its conversation; `id` opens in
    /// a new tab beside it.
    fn switch_conversation(
        &mut self,
        id: &str,
        open_elsewhere: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(panel) = self.active_chat.clone() else {
            return;
        };
        if let Some(shown) = self.chat_showing(id, cx) {
            self.show_chat_panel(&shown, window, cx);
            return;
        }
        let project = panel.read(cx).project().clone();
        let mut chat = panel.read(cx).chat().clone();
        if chat.read(cx).status() == Status::Running {
            chat = self.add_chat(Place::Tab, &project, window, cx);
        }
        chat.update(cx, |chat, cx| {
            chat.open_session(id, open_elsewhere, window, cx);
        });
        self.sync_titles(&project, cx);
        focus(&chat, window, cx);
    }

    /// Shows `panel`'s tab and focuses its chat.
    pub(super) fn show_chat_panel(
        &mut self,
        panel: &Entity<ChatPanel>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = PanelId::from(panel.entity_id());
        self.dock
            .update(cx, |dock, cx| dock.select_panel(id, window, cx));
        self.activate(Some(panel.clone()), window, cx);
        self.focus_active_chat(window, cx);
    }

    pub(super) fn focus_active_chat(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = &self.active_chat {
            let chat = panel.read(cx).chat().clone();
            focus(&chat, window, cx);
        }
    }

    /// An open chat in `project`.
    pub(super) fn chats_in(
        &self,
        project: &Entity<Project>,
        cx: &gpui_kit::App,
    ) -> Option<Entity<ChatPanel>> {
        let mut panels = self.chats.iter().map(|tab| &tab.panel);
        panels
            .find(|panel| panel.read(cx).project() == project)
            .cloned()
    }

    fn on_chat_panel(
        &mut self,
        panel: &Entity<ChatPanel>,
        event: &ChatPanelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, ChatPanelEvent::Removed) {
            self.remove_chat(panel, window, cx);
            return;
        }
        self.activate(Some(panel.clone()), window, cx);
        let place = match event {
            ChatPanelEvent::Open { id, open_elsewhere } => {
                return self.switch_conversation(id, *open_elsewhere, window, cx);
            }
            ChatPanelEvent::OpenProject(folder) => return self.open_folder(folder, window, cx),
            ChatPanelEvent::BrowseProjects => return self.prompt_open_folder(window, cx),
            ChatPanelEvent::NewWorktree => return self.new_worktree_chat(panel, window, cx),
            ChatPanelEvent::Close => return self.close_chat(panel, window, cx),
            ChatPanelEvent::Activated | ChatPanelEvent::Removed => return,
            ChatPanelEvent::NewTab => Place::Tab,
            ChatPanelEvent::SplitRight => Place::Right,
        };
        let project = panel.read(cx).project().clone();
        let chat = self.add_chat(place, &project, window, cx);
        focus(&chat, window, cx);
    }

    /// Moves on to the tab now shown in the active chat's group, so the side
    /// panel never shows the folder of a chat that is out of sight.
    pub(super) fn follow_dock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(group) = self.focused_group(cx) else {
            return;
        };
        let dock = self.dock.read(cx);
        let shown = dock
            .layout(DockPlacement::Center)
            .and_then(|tree| displayed(tree.root(), Some(group)))
            .and_then(|id| self.chat_in(id));
        if shown.is_some() && shown != self.active_chat {
            self.activate(shown, window, cx);
        }
    }

    fn chat_in(&self, id: PanelId) -> Option<Entity<ChatPanel>> {
        let mut panels = self.chats.iter().map(|tab| &tab.panel);
        panels
            .find(|panel| PanelId::from(panel.entity_id()) == id)
            .cloned()
    }

    /// The chat the first group of tabs shows.
    pub(super) fn displayed_chat(&self, cx: &gpui_kit::App) -> Option<Entity<ChatPanel>> {
        let dock = self.dock.read(cx);
        let tree = dock.layout(DockPlacement::Center)?;
        self.chat_in(displayed(tree.root(), None)?)
    }

    fn on_chat(
        &mut self,
        panel: &WeakEntity<ChatPanel>,
        event: &ChatEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_running(cx);
        let Some(panel) = panel.upgrade() else {
            return;
        };
        match event {
            ChatEvent::SessionChanged => {
                // Its turn ended, or it shows another conversation.
                self.refresh_project(window, cx);
                let (project, backend) = (panel.read(cx).project().clone(), self.backend.clone());
                project.update(cx, |project, cx| {
                    project.reload_sessions(backend.as_ref(), cx);
                });
                self.save_layout(cx);
            }
            ChatEvent::AccountChanged | ChatEvent::ChoicesChanged => self.save_layout(cx),
            ChatEvent::OpenFile(path) => {
                self.activate(Some(panel), window, cx);
                self.open_absolute(path.clone(), window, cx);
            }
            ChatEvent::PickProject => {
                self.activate(Some(panel), window, cx);
                self.pick_project(window, cx);
            }
            ChatEvent::TurnStarted | ChatEvent::TurnCompleted | ChatEvent::PickConversation => {}
        }
    }

    /// Tells the conversations pages which conversations have a turn
    /// running.
    pub(super) fn sync_running(&mut self, cx: &mut Context<Self>) {
        let running: HashSet<String> = self
            .chats
            .iter()
            .map(|tab| tab.panel.read(cx).chat().read(cx))
            .filter(|chat| chat.status() == Status::Running)
            .filter_map(|chat| chat.session_id().map(str::to_owned))
            .collect();
        for (project, _) in &self.projects {
            let sessions = project.read(cx).sessions().clone();
            sessions.update(cx, |sessions, cx| sessions.set_running(running.clone(), cx));
        }
    }
}

/// The panel shown by `group`, or by the first group there is.
fn displayed(node: &PaneNode, group: Option<NodeId>) -> Option<PanelId> {
    match node.kind() {
        PaneRef::Split { children, .. } => {
            children.iter().find_map(|child| displayed(child, group))
        }
        PaneRef::Tabs { panels, active_ix } => group
            .is_none_or(|group| group == node.id())
            .then(|| panels.get(active_ix).copied())
            .flatten(),
    }
}

pub(super) fn focus(chat: &Entity<ChatView>, window: &mut Window, cx: &mut gpui_kit::App) {
    chat.read(cx).focus_handle(cx).focus(window, cx);
}
