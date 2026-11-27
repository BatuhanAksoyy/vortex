//! Each harness's CLI version in the status bar, and moving its open chats
//! onto the CLI installed now, after an update: each restarts and resumes
//! its conversation.

use gpui_kit::{
    App, Context, Entity, PathPromptOptions, Subscription, Window,
    component::{WindowExt as _, notification::Notification},
};
use serde_json::Value;
use vortex_chat::ChatView;
use vortex_settings::Scope;

use super::Workspace;
use crate::cli_version::CliVersion;

pub(super) struct ShownVersion {
    /// The harness whose CLI it is, by its settings name.
    pub(super) agent: &'static str,
    pub(super) view: Entity<CliVersion>,
    pub(super) _restart: Subscription,
}

impl Workspace {
    /// The version of the active chat's harness. None shows without a
    /// chat: until one picks its harness, which CLI is there is no news.
    pub(super) fn shown_cli_version(&self, cx: &App) -> Option<Entity<CliVersion>> {
        let active = self.active_chat.as_ref()?;
        let agent = active.read(cx).chat().read(cx).agent_name();
        let own = self.cli_versions.iter().find(|shown| shown.agent == agent);
        own.map(|shown| shown.view.clone())
    }

    /// The chats on harness `agent` whose process runs a version other
    /// than `installed`.
    pub(super) fn chats_behind(
        &self,
        agent: &str,
        installed: &str,
        cx: &App,
    ) -> Vec<Entity<ChatView>> {
        let chats = self.chats.iter().map(|tab| tab.panel.read(cx).chat());
        chats
            .filter(|chat| {
                let chat = chat.read(cx);
                chat.agent_name() == agent && chat.cli_version().is_some_and(|run| run != installed)
            })
            .cloned()
            .collect()
    }

    /// Installs the CLI of harness `agent`, missing here.
    pub(super) fn install_cli(&self, agent: &str, cx: &mut Context<Self>) {
        let shown = self.cli_versions.iter().find(|shown| shown.agent == agent);
        if let Some(shown) = shown {
            shown.view.update(cx, CliVersion::start_install);
        }
    }

    /// Asks where the CLI of harness `agent` is and keeps it in the settings
    /// for all projects, as the settings page would; the CLI is then looked
    /// at again, and a chat waiting on it starts.
    pub(super) fn choose_cli_path(agent: &str, window: &mut Window, cx: &mut Context<Self>) {
        let key = match agent {
            "claude" => ["agents", "claude_path"],
            "codex" => ["agents", "codex_path"],
            _ => return,
        };
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let value = Value::String(path.display().to_string());
            let written = vortex_settings::Layers::read(None, Value::Null)
                .and_then(|mut layers| layers.set(Scope::Global, &key, Some(value)));
            let _ = this.update_in(cx, |this, window, cx| match written {
                Ok(()) => this.reload_config(window, cx),
                Err(error) => {
                    let error = format!("Cannot keep the path: {error:#}");
                    window.push_notification(Notification::error(error), cx);
                }
            });
        })
        .detach();
    }

    /// Tells every chat on harness `agent` whether its CLI is missing; one
    /// whose CLI arrived starts its process.
    pub(super) fn cli_setup(
        &mut self,
        agent: &str,
        setup: Option<vortex_chat::Setup>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if vortex_chat::AgentSetup::of(agent, cx) == setup {
            return;
        }
        vortex_chat::AgentSetup::set(agent, setup, cx);
        let chats: Vec<_> = (self.chats.iter())
            .map(|tab| tab.panel.read(cx).chat().clone())
            .collect();
        for chat in chats {
            chat.update(cx, |chat, cx| chat.cli_checked(agent, window, cx));
        }
        cx.notify();
    }

    /// Moves the chats of harness `agent` left on an older CLI onto the
    /// installed one.
    pub(super) fn restart_chats_behind(
        &mut self,
        agent: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let shown = self.cli_versions.iter().find(|shown| shown.agent == agent);
        let Some(installed) = shown.and_then(|shown| shown.view.read(cx).installed()) else {
            return;
        };
        for chat in self.chats_behind(agent, &installed, cx) {
            chat.update(cx, |chat, cx| chat.restart_on_new_cli(window, cx));
        }
    }
}
