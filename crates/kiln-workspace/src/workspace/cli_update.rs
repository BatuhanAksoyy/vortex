//! Each harness's CLI version in the status bar, and moving its open chats
//! onto the CLI installed now, after an update: each restarts and resumes
//! its conversation.

use gpui_kit::{App, Context, Entity, Subscription, Window};
use kiln_chat::ChatView;

use super::Workspace;
use crate::cli_version::CliVersion;

pub(super) struct ShownVersion {
    /// The harness whose CLI it is, by its settings name.
    pub(super) agent: &'static str,
    pub(super) view: Entity<CliVersion>,
    pub(super) _restart: Subscription,
}

impl Workspace {
    /// The version of the active chat's harness, else the first one's.
    pub(super) fn shown_cli_version(&self, cx: &App) -> Option<Entity<CliVersion>> {
        let active =
            (self.active_chat.as_ref()).map(|panel| panel.read(cx).chat().read(cx).agent_name());
        let own = self
            .cli_versions
            .iter()
            .find(|shown| Some(shown.agent) == active);
        own.or(self.cli_versions.first())
            .map(|shown| shown.view.clone())
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

    /// Tells every chat on harness `agent` whether its CLI is missing; one
    /// whose CLI arrived starts its process.
    pub(super) fn cli_setup(
        &mut self,
        agent: &str,
        setup: Option<kiln_chat::Setup>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if kiln_chat::AgentSetup::of(agent, cx) == setup {
            return;
        }
        kiln_chat::AgentSetup::set(agent, setup, cx);
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
