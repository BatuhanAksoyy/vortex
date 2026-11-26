//! Moving open chats onto the Claude Code installed now, after an update:
//! each restarts and resumes its conversation.

use gpui_kit::{Context, Entity, Window};
use kiln_chat::ChatView;

use super::Workspace;

impl Workspace {
    /// The chats whose process runs a version other than `installed`.
    pub(super) fn chats_behind(
        &self,
        installed: &str,
        cx: &gpui_kit::App,
    ) -> Vec<Entity<ChatView>> {
        let chats = self.chats.iter().map(|tab| tab.panel.read(cx).chat());
        chats
            .filter(|chat| {
                chat.read(cx)
                    .cli_version()
                    .is_some_and(|run| run != installed)
            })
            .cloned()
            .collect()
    }

    /// Moves the chats left on an older CLI onto the installed one.
    pub(super) fn restart_chats_behind(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let installed = self
            .cli_version
            .as_ref()
            .and_then(|(version, _)| version.read(cx).installed());
        let Some(installed) = installed else {
            return;
        };
        for chat in self.chats_behind(&installed, cx) {
            chat.update(cx, |chat, cx| chat.restart_on_new_cli(window, cx));
        }
    }
}
