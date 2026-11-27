//! Closing chat tabs. Any tab closes, the last one too; the window then
//! offers the folders to start in.

use gpui_kit::{Context, Entity, Window};

use super::Workspace;
use crate::chat_panel::ChatPanel;

impl Workspace {
    /// Closes `panel`'s tab; the dock tells it so, and its process ends.
    pub(super) fn close_chat(
        &mut self,
        panel: &Entity<ChatPanel>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dock
            .update(cx, |dock, cx| dock.remove_panel(panel.clone(), window, cx));
    }

    /// Forgets a closed tab, which ends its process. With none left, the
    /// window offers the folders to start in.
    pub(super) fn remove_chat(
        &mut self,
        panel: &Entity<ChatPanel>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ix) = self.chats.iter().position(|tab| &tab.panel == panel) else {
            return;
        };
        self.chats.remove(ix);
        self.browsers.retain(|(chat, _)| *chat != panel.entity_id());
        self.sync_alone(cx);
        if self.active_chat.as_ref() == Some(panel) {
            let next = self.displayed_chat(cx);
            self.activate(next, window, cx);
        }
        if self.chats.is_empty() {
            self.show_start_page(window, cx);
        }
        self.sync_running(cx);
        self.save_layout(cx);
    }

    /// Tells the only tab it is the last, as it then draws its own close
    /// button.
    pub(super) fn sync_alone(&self, cx: &mut Context<Self>) {
        let alone = self.chats.len() == 1;
        for tab in &self.chats {
            tab.panel.update(cx, |panel, cx| panel.set_alone(alone, cx));
        }
    }
}
