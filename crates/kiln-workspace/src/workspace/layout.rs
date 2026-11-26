//! Keeping the chat layout across restarts: which conversations are open,
//! in which groups, split how.

use gpui_kit::{
    App, Axis, Context, Entity, Window,
    component::dock::{DockLayout, DockPlacement, PaneNode, PaneRef, panel_handle},
    px,
};

use kiln_chat::ChatView;

use super::Workspace;
use crate::{
    chat_panel::ChatPanel,
    saved_layout::{Pane, SavedChat, Slot},
};

impl Workspace {
    /// Opens the layout kept for the window once its conversations are
    /// listed, then the folder Kiln was started in.
    pub(super) fn start_chats(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.serve_agent_tools(window, cx);
        let folder = self.first_folder.clone();
        let kept = self.layouts.as_ref();
        let pane = kept.and_then(|file| file.load(folder.as_deref()));
        let Some(pane) = pane else {
            self.start_in_folder(window, cx);
            return;
        };
        // Kept until the listings say which conversations still exist and
        // which are open in another process, so none is resumed blindly.
        for folder in pane.folders() {
            self.project_at(&folder, window, cx);
        }
        self.pending_layout = Some(pane);
        self.restore_layout(window, cx);
    }

    /// Opens the kept layout, once every project's conversations are listed.
    pub(super) fn restore_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let projects = self.projects.iter().map(|(project, _)| project.read(cx));
        if !projects.into_iter().all(|p| p.sessions().read(cx).loaded()) {
            return;
        }
        let Some(pane) = self.pending_layout.take() else {
            return;
        };
        if let Some((layout, shown)) = self.build(&pane, window, cx) {
            // The center's root must be a split.
            let layout = match pane {
                Pane::Split { .. } => layout,
                Pane::Tabs { .. } => DockLayout::h_split().child(layout, None),
            };
            self.dock
                .update(cx, |dock, cx| dock.set_center(layout, window, cx));
            self.activate(Some(shown), window, cx);
            self.sync_running(cx);
        }
        self.start_in_folder(window, cx);
    }

    /// The dock layout for `pane` with a chat resuming each conversation
    /// still listed, and the chat its first group displays.
    fn build(
        &mut self,
        pane: &Pane,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<(DockLayout, Entity<ChatPanel>)> {
        let (vertical, children) = match pane {
            Pane::Split { vertical, children } => (*vertical, children),
            Pane::Tabs { active, chats } => {
                return self.build_tabs(*active, chats, window, cx);
            }
        };
        let built: Vec<_> = children
            .iter()
            .filter_map(|slot| {
                let (layout, shown) = self.build(&slot.pane, window, cx)?;
                Some((layout, slot.size, shown))
            })
            .collect();
        let shown = built.first()?.2.clone();
        let split = if vertical {
            DockLayout::v_split()
        } else {
            DockLayout::h_split()
        };
        let split = built.into_iter().fold(split, |split, (layout, size, _)| {
            split.child(layout, size.map(px))
        });
        Some((split, shown))
    }

    fn build_tabs(
        &mut self,
        active: usize,
        chats: &[SavedChat],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<(DockLayout, Entity<ChatPanel>)> {
        let mut listed = Vec::new();
        for saved in chats {
            let Some(folder) = saved.folder.as_deref().filter(|folder| folder.is_dir()) else {
                continue;
            };
            let project = self.project_at(folder, window, cx);
            let sessions = project.read(cx).sessions().read(cx);
            // A conversation no longer listed was deleted; a blank tab
            // comes back blank.
            let open_elsewhere = match &saved.session {
                Some(id) => sessions.open_elsewhere(id),
                None => Some(false),
            };
            if let Some(open_elsewhere) = open_elsewhere {
                listed.push((saved, open_elsewhere, project));
            }
        }
        let displayed = chats.get(active);
        let active = listed
            .iter()
            .position(|(saved, ..)| Some(*saved) == displayed)
            .unwrap_or_default();
        let mut tabs = DockLayout::tabs();
        let mut shown = None;
        for (ix, (saved, open_elsewhere, project)) in listed.into_iter().enumerate() {
            let panel = self.new_chat_panel(&project, window, cx);
            let chat = panel.read(cx).chat().clone();
            chat.update(cx, |chat, cx| {
                resume(chat, saved, open_elsewhere, window, cx);
            });
            if ix == active {
                shown = Some(panel.clone());
            }
            tabs = tabs.panel_view(panel_handle(panel), cx);
        }
        Some((tabs.active_index(active), shown?))
    }

    /// Writes the layout down for the next start.
    pub(super) fn save_layout(&self, cx: &App) {
        // Until restored, the dock does not show the kept layout yet.
        if self.pending_layout.is_some() {
            return;
        }
        let Some(file) = &self.layouts else {
            return;
        };
        let pane = self
            .dock
            .read(cx)
            .layout(DockPlacement::Center)
            .map(|tree| self.saved_pane(tree.root(), cx))
            .and_then(Pane::pruned);
        if let Err(error) = file.save(pane) {
            log::warn!("cannot keep the chat layout: {error:#}");
        }
    }

    fn saved_pane(&self, node: &PaneNode, cx: &App) -> Pane {
        match node.kind() {
            PaneRef::Split {
                axis,
                children,
                sizes,
            } => Pane::Split {
                vertical: axis == Axis::Vertical,
                children: children
                    .iter()
                    .zip(sizes)
                    .map(|(child, size)| Slot {
                        pane: self.saved_pane(child, cx),
                        size: size.map(f32::from),
                    })
                    .collect(),
            },
            PaneRef::Tabs { panels, active_ix } => {
                let displayed = panels
                    .get(active_ix)
                    .and_then(|id| self.saved_chat_in(*id, cx));
                let chats: Vec<SavedChat> = panels
                    .iter()
                    .filter_map(|id| self.saved_chat_in(*id, cx))
                    .collect();
                let active = chats
                    .iter()
                    .position(|saved| Some(saved) == displayed.as_ref())
                    .unwrap_or_default();
                Pane::Tabs { active, chats }
            }
        }
    }
}

/// Opens `saved` in `chat`, with its account, mode and effort from the first
/// process on.
fn resume(
    chat: &mut ChatView,
    saved: &SavedChat,
    open_elsewhere: bool,
    window: &mut Window,
    cx: &mut Context<ChatView>,
) {
    if let Some(account) = &saved.account {
        chat.start_with_account(account, cx);
    }
    if let Some(mode) = saved.mode {
        chat.start_in_mode(mode, cx);
    }
    if let Some(effort) = saved.effort {
        chat.start_with_effort(effort, cx);
    }
    if let Some(id) = &saved.session {
        chat.open_session(id, open_elsewhere, window, cx);
    }
}
