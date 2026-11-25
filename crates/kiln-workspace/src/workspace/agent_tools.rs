//! Kiln's tools for the chats' agents. Each chat's agent gets a token of its
//! own, and a call carrying it goes to that chat's browser.

use std::path::Path;

use async_channel::Receiver;
use gpui_kit::{Context, Entity, Task, WeakEntity, Window};
use kiln_bridge::{Bridge, Call, Reply};

use super::Workspace;
use crate::{browser::Browser, chat_panel::ChatPanel};

pub(super) struct AgentTools {
    bridge: Bridge,
    /// Until the window serves them.
    calls: Option<Receiver<Call>>,
    chats: Vec<(String, WeakEntity<ChatPanel>)>,
    serving: Task<()>,
}

impl AgentTools {
    /// Listens beside the app's data in `dir`; without it, agents get no
    /// tools.
    pub(super) fn start(dir: &Path) -> Option<Self> {
        let started = std::env::current_exe()
            .and_then(|program| Bridge::start(dir, program).map_err(std::io::Error::other));
        match started {
            Ok((bridge, calls)) => Some(Self {
                bridge,
                calls: Some(calls),
                chats: Vec::new(),
                serving: Task::ready(()),
            }),
            Err(error) => {
                log::warn!("agents get no browser tools: {error:#}");
                None
            }
        }
    }
}

impl Workspace {
    pub(super) fn serve_agent_tools(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tools) = &mut self.agent_tools else {
            return;
        };
        let Some(calls) = tools.calls.take() else {
            return;
        };
        tools.serving = cx.spawn_in(window, async move |this, cx| {
            while let Ok(call) = calls.recv().await
                && this
                    .update_in(cx, |this, window, cx| this.on_tool_call(call, window, cx))
                    .is_ok()
            {}
        });
    }

    /// Gives the chat's agent the tools, before its first process starts.
    pub(super) fn give_tools(&mut self, panel: &Entity<ChatPanel>, cx: &mut Context<Self>) {
        let Some(tools) = &mut self.agent_tools else {
            return;
        };
        match tools.bridge.server() {
            Ok((server, token)) => {
                tools.chats.push((token, panel.downgrade()));
                let chat = panel.read(cx).chat().clone();
                chat.update(cx, |chat, _| chat.set_tools(server));
            }
            Err(error) => log::warn!("a chat gets no browser tools: {error:#}"),
        }
    }

    fn on_tool_call(&mut self, call: Call, window: &mut Window, cx: &mut Context<Self>) {
        let chats = self
            .agent_tools
            .as_ref()
            .map(|tools| tools.chats.as_slice());
        let panel = chats
            .unwrap_or_default()
            .iter()
            .find(|(token, _)| *token == call.token)
            .and_then(|(_, panel)| panel.upgrade());
        let Some(panel) = panel else {
            let _ = call.reply.try_send(Reply::failed("The chat is closed."));
            return;
        };
        let browser = self.agent_browser(&panel, window, cx);
        browser.update(cx, |browser, cx| {
            browser.serve(call.request, call.reply, window, cx);
        });
    }

    /// The chat's browser, shown if the chat is the one the user is in.
    fn agent_browser(
        &mut self,
        panel: &Entity<ChatPanel>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<Browser> {
        let id = panel.entity_id();
        let kept = self.browsers.iter().find(|(chat, _)| *chat == id);
        let browser = if let Some((_, browser)) = kept {
            browser.clone()
        } else {
            let browser = self.new_browser(panel, window, cx);
            self.browsers.push((id, browser.clone()));
            browser
        };
        let active = self.active_chat.as_ref() == Some(panel);
        if active && !self.side_panel.browser_open {
            // So the user sees what the agent does; the keyboard stays put.
            self.side_panel.browser_open = true;
            self.uncover_chats();
            self.save_side_panel(cx);
            self.sync_browser(window, cx);
        }
        browser
    }
}
