//! Switching a chat to another harness while it holds no conversation yet.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, Context, Window,
    component::{
        Selectable as _,
        button::{Button, ButtonGroup},
    },
};

use super::{ChatEvent, ChatView};
use crate::{
    agent_state::AgentState,
    backends::{Backends, title},
};

impl ChatView {
    /// Offers `backends` to pick from while the chat is blank.
    #[must_use]
    pub fn offering(mut self, backends: Backends) -> Self {
        self.offer(backends);
        self
    }

    pub fn offer(&mut self, backends: Backends) {
        self.backends = Some(backends);
    }

    /// Runs the chat on harness `name` from here on: its process restarts
    /// with that harness, on its first added account. Only while the chat
    /// shows no conversation, which the other harness could not continue.
    pub fn use_agent(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.conversation.is_some() || self.transcript.len() > 0 {
            return;
        }
        if self.switch_backend(name, cx) {
            self.ensure_session(window, cx);
            cx.emit(ChatEvent::AccountChanged);
            cx.notify();
        }
    }

    /// Swaps in harness `name` and its first added account, with the
    /// process stopped; whether it was another harness.
    pub(super) fn switch_backend(&mut self, name: &str, cx: &mut Context<Self>) -> bool {
        let Some(backend) = self.backends.as_ref().map(|backends| backends.get(name)) else {
            return false;
        };
        if backend.name() == self.backend.name() {
            return false;
        }
        self.backend = backend;
        self.session = None;
        self.agent = AgentState::default();
        self.account = super::first_added(self.accounts.read(cx), self.backend.name());
        self.recall_usage(cx);
        true
    }

    /// The chat's harness and then the others it may hand over to.
    pub(super) fn offered_agents(&self) -> Vec<&'static str> {
        let own = self.backend.name();
        let others = self.backends.iter().flat_map(Backends::iter);
        let others = others
            .map(|backend| backend.name())
            .filter(|name| *name != own);
        std::iter::once(own).chain(others).collect()
    }

    /// What the empty chat calls the agent: Claude, not Claude Code.
    pub(crate) fn agent_short_name(&self) -> &'static str {
        match self.backend.name() {
            "codex" => "Codex",
            _ => "Claude",
        }
    }

    /// The harness picker a blank chat shows; `None` with a single harness.
    pub(crate) fn render_agent_picker(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let backends = self
            .backends
            .as_ref()
            .filter(|backends| backends.several())?;
        let current = self.backend.name();
        let names: Vec<&'static str> = backends.iter().map(|backend| backend.name()).collect();
        let buttons = names.iter().map(|name| {
            Button::new(gpui_kit::SharedString::from(format!("chat-agent-{name}")))
                .label(title(name).to_owned())
                .selected(*name == current)
        });
        let picker = ButtonGroup::new("chat-agent-picker")
            .outline()
            .children(buttons)
            .on_click(cx.listener(move |this, clicked: &Vec<usize>, window, cx| {
                if let Some(name) = clicked.first().and_then(|ix| names.get(*ix)) {
                    this.use_agent(name, window, cx);
                }
            }));
        Some(picker.into_any_element())
    }
}
