//! The settings a chat works with: its project's, with the chat's own
//! choices over them. The chat's own go with it when Kiln keeps the layout.

use gpui_kit::{App, Context};
use kiln_settings::Settings;
use serde_json::Value;

use super::{ChatPanel, ChatPanelEvent};

impl ChatPanel {
    /// The project's settings with the chat's own over them.
    #[must_use]
    pub fn settings(&self, cx: &App) -> Settings {
        let root = self.project.read(cx).root();
        let Some(project) = crate::config::project_settings(root, cx) else {
            return crate::config::settings(cx).clone();
        };
        kiln_settings::with_session(&project.merged, &self.session).unwrap_or_else(|error| {
            log::warn!("the chat's own settings do not apply: {error:#}");
            project.settings.clone()
        })
    }

    /// The settings chosen for this chat alone.
    #[must_use]
    pub fn session_settings(&self) -> &Value {
        &self.session
    }

    /// Sets `key` for this chat alone; `None` goes back to the project's.
    pub fn set_session_setting(
        &mut self,
        key: &[&str],
        value: Option<Value>,
        cx: &mut Context<Self>,
    ) {
        kiln_settings::set_in_value(&mut self.session, key, value);
        self.settings_changed(cx);
        cx.emit(ChatPanelEvent::SettingsChanged);
    }

    /// The chat's own settings as Kiln kept them.
    pub fn restore_session_settings(&mut self, session: Value, cx: &mut Context<Self>) {
        self.session = session;
        self.settings_changed(cx);
    }

    /// Applies what the chat's view takes from the settings.
    pub fn settings_changed(&mut self, cx: &mut Context<Self>) {
        let settings = self.settings(cx);
        let sleep_after = settings.chat.sleep_after();
        let directives = settings.git.commit.instructions;
        self.chat.update(cx, |chat, cx| {
            chat.set_sleep_after(sleep_after, cx);
            chat.set_commit_directives(directives);
        });
        cx.notify();
    }
}
