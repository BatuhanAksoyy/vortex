//! The agent process behind the chat: starting it, folding its events in,
//! and the commands the composer sends (model, mode, context usage).

use gpui_kit::prelude::*;
use std::time::SystemTime;

use gpui_kit::{Context, Window};
use kiln_agent::{AgentEvent, PermissionMode, SessionCommand, SessionSpec};

use super::{ChatView, LiveSession};

const USAGE_POLL: std::time::Duration = std::time::Duration::from_mins(5);
use crate::agent_state::{AgentState, next_mode};

impl ChatView {
    pub fn set_model(&mut self, id: &str, cx: &mut Context<Self>) {
        self.agent.choose_model(id);
        self.send_command(SessionCommand::SetModel(id.to_owned()));
        cx.notify();
    }

    /// Asks the agent to switch modes. The composer follows when the agent
    /// confirms; without a process the choice is kept for the next one.
    pub fn set_mode(&mut self, mode: PermissionMode, cx: &mut Context<Self>) {
        if !self.send_command(SessionCommand::SetMode(mode)) {
            self.agent.apply(&AgentEvent::ModeChanged(mode));
        }
        cx.notify();
    }

    pub fn cycle_mode(&mut self, cx: &mut Context<Self>) {
        self.set_mode(next_mode(self.agent.mode()), cx);
    }

    #[must_use]
    pub fn mode(&self) -> PermissionMode {
        self.agent.mode()
    }

    /// Asks for fresh plan usage; usage spent elsewhere (another session,
    /// claude.ai) shows only when asked for.
    pub fn refresh_usage(&mut self, _: &mut Context<Self>) {
        self.send_command(SessionCommand::RefreshUsage);
    }

    /// Keeps the header's usage current while the chat sits idle.
    pub(super) async fn poll_usage(this: gpui_kit::WeakEntity<Self>, cx: &mut gpui_kit::AsyncApp) {
        let mut alive = true;
        while alive {
            cx.background_executor().timer(USAGE_POLL).await;
            alive = this.update(cx, Self::refresh_usage).is_ok();
        }
    }

    /// Whether a usage limit stops messages now.
    #[must_use]
    pub fn is_limited(&self) -> bool {
        self.agent.limit_at(SystemTime::now()).is_some()
    }

    /// Waits for the limit's reset time, then lifts it and asks for fresh
    /// usage. A limit without a reset time lasts until the agent reports
    /// otherwise.
    fn sync_limit_timer(&mut self, cx: &mut Context<Self>) {
        let wait = self
            .agent
            .limit_at(SystemTime::now())
            .and_then(|limited| limited.until)
            .and_then(|until| until.duration_since(SystemTime::now()).ok());
        self.limit_timer = wait.map(|wait| {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(wait).await;
                let _ = this.update(cx, Self::end_limit);
            })
        });
    }

    fn end_limit(&mut self, cx: &mut Context<Self>) {
        self.agent.end_limit();
        self.send_command(SessionCommand::RefreshUsage);
        cx.notify();
    }

    /// Sends `command` to the running process; `false` if there is none.
    pub(super) fn send_command(&self, command: SessionCommand) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| session.commands.try_send(command).is_ok())
    }

    /// The running session's command channel, starting a session if needed.
    pub(super) fn ensure_session(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<async_channel::Sender<SessionCommand>> {
        if self.session.is_none() {
            match self.start_session(window, cx) {
                Ok(session) => self.session = Some(session),
                Err(error) => {
                    let change = self.transcript.apply(AgentEvent::Exited {
                        error: Some(format!("{error:#}")),
                    });
                    self.apply_change(change, cx);
                    return None;
                }
            }
        }
        self.session
            .as_ref()
            .map(|session| session.commands.clone())
    }

    fn start_session(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<LiveSession> {
        let session = self.backend.spawn(SessionSpec {
            cwd: self.cwd.clone(),
            env: Vec::new(),
            model: self.agent.chosen_model().map(str::to_owned),
            mode: self.agent.start_mode(),
        })?;
        let events = session.events;
        let events_task = cx.spawn_in(window, async move |this, cx| {
            while let Ok(event) = events.recv().await
                && this
                    .update_in(cx, |this, window, cx| this.on_event(event, window, cx))
                    .is_ok()
            {}
        });
        Ok(LiveSession {
            commands: session.commands,
            _driver: cx.background_spawn(session.driver),
            _events: events_task,
        })
    }

    fn on_event(&mut self, event: AgentEvent, window: &mut Window, cx: &mut Context<Self>) {
        for command in AgentState::refreshes_after(&event) {
            self.send_command(command.clone());
        }
        if self.agent.apply(&event) {
            cx.notify();
        }
        if matches!(event, AgentEvent::Usage(_)) {
            self.sync_limit_timer(cx);
        }
        self.update_skills(&event, cx);
        let exited = matches!(event, AgentEvent::Exited { .. });
        if event == AgentEvent::ConversationReset {
            self.toggled.clear();
            self.scroller
                .update(cx, |scroller, cx| scroller.reset(1, cx));
        }
        let change = self.transcript.apply(event);
        self.apply_change(change, cx);
        if exited {
            self.session = None;
        }
        self.sync_prompt(window, cx);
    }
}
