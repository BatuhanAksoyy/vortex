//! Keeping the chat layout across restarts: which conversations are open,
//! in which groups, split how.

use gpui_kit::{
    App, Axis, Context, Entity, Window,
    component::dock::{DockLayout, DockPlacement, PaneNode, PaneRef, panel_handle},
    px,
};

use kiln_chat::ChatView;

use super::{Workspace, chats::Place};
use crate::{
    chat_panel::ChatPanel,
    saved_layout::{Pane, SavedChat, Slot},
};

impl Workspace {
    /// Starts the project's chats: the layout kept for it once its
    /// conversations are listed, otherwise one new chat.
    pub(super) fn start_chats(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_chats(window, cx);
        // Kept until the listing says which conversations still exist and
        // which are open in another process, so none is resumed blindly.
        self.pending_layout = self.layouts.as_ref().and_then(|file| file.load(&self.root));
        if self.pending_layout.is_none() {
            self.add_chat(Place::Tab, window, cx);
        }
    }

    /// Opens the kept layout, once the session list has loaded.
    pub(super) fn restore_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pane) = self.pending_layout.take() else {
            return;
        };
        let Some((layout, shown)) = self.build(&pane, window, cx) else {
            self.add_chat(Place::Tab, window, cx);
            return;
        };
        // The center's root must be a split.
        let layout = match pane {
            Pane::Split { .. } => layout,
            Pane::Tabs { .. } => DockLayout::h_split().child(layout, None),
        };
        self.dock
            .update(cx, |dock, cx| dock.set_center(layout, window, cx));
        self.active_chat = Some(shown);
        self.sync_sidebar(cx);
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
        let listed: Vec<(&SavedChat, bool)> = chats
            .iter()
            .filter_map(|saved| {
                let open_elsewhere = self.sessions.read(cx).open_elsewhere(&saved.session)?;
                Some((saved, open_elsewhere))
            })
            .collect();
        let displayed = chats.get(active);
        let active = listed
            .iter()
            .position(|(saved, _)| Some(*saved) == displayed)
            .unwrap_or_default();
        let mut tabs = DockLayout::tabs();
        let mut shown = None;
        for (ix, (saved, open_elsewhere)) in listed.into_iter().enumerate() {
            let panel = self.new_chat_panel(window, cx);
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
        if let Err(error) = file.save(&self.root, pane) {
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
    chat.open_session(&saved.session, open_elsewhere, window, cx);
}
