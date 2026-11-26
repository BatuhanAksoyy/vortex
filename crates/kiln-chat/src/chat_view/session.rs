//! The agent process behind the chat: starting it, folding its events in,
//! and the commands the composer sends (model, mode, effort, context usage).

use gpui_kit::prelude::*;
use std::time::SystemTime;

use gpui_kit::{Context, Window};
use kiln_agent::{AgentEvent, Effort, PermissionMode, SessionCommand, SessionSpec, ToolServer};

use super::{ChatView, LiveSession};

const USAGE_POLL: std::time::Duration = std::time::Duration::from_mins(2);
use crate::agent_state::{AgentState, next_mode};

impl ChatView {
    /// Gives the chat's agent Kiln's tools from its next process on; call it
    /// before the first, which starts once the chat is made.
    pub fn set_tools(&mut self, server: ToolServer) {
        self.agent.tools = Some(server);
    }

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
            cx.emit(super::ChatEvent::ChoicesChanged);
        }
        cx.notify();
    }

    /// Asks the agent for `effort`, as for a mode.
    pub fn set_effort(&mut self, effort: Effort, cx: &mut Context<Self>) {
        if !self.send_command(SessionCommand::SetEffort(effort)) {
            self.agent.apply(&AgentEvent::EffortChanged(Some(effort)));
            cx.emit(super::ChatEvent::ChoicesChanged);
        }
        cx.notify();
    }

    /// The effort the agent reported or the user chose; `None` until then,
    /// while the user's own settings decide it.
    #[must_use]
    pub fn known_effort(&self) -> Option<Effort> {
        self.agent.start_effort()
    }

    /// Runs the chat with `effort` from its first process, as with
    /// [`Self::start_in_mode`].
    pub fn start_with_effort(&mut self, effort: Effort, cx: &mut Context<Self>) {
        if self.session.is_none() {
            self.agent.apply(&AgentEvent::EffortChanged(Some(effort)));
            cx.notify();
        }
    }

    pub fn cycle_mode(&mut self, cx: &mut Context<Self>) {
        self.set_mode(next_mode(self.agent.mode()), cx);
    }

    #[must_use]
    pub fn mode(&self) -> PermissionMode {
        self.agent.mode()
    }

    /// The mode the agent reported or the user chose; `None` until then,
    /// while the user's own settings decide it. Never bypassing permissions:
    /// this is kept for the chat to reopen in, and that one is confirmed
    /// anew.
    #[must_use]
    pub fn known_mode(&self) -> Option<PermissionMode> {
        self.agent
            .start_mode()
            .filter(|mode| *mode != PermissionMode::BypassPermissions)
    }

    /// Runs the chat in `mode` from its first process, as when a kept
    /// layout reopens it. Ignored once a process has started.
    pub fn start_in_mode(&mut self, mode: PermissionMode, cx: &mut Context<Self>) {
        if self.session.is_none() {
            self.agent.apply(&AgentEvent::ModeChanged(mode));
            cx.notify();
        }
    }

    /// Asks for fresh plan usage; usage spent elsewhere (another session,
    /// claude.ai) shows only when asked for.
    pub fn refresh_usage(&mut self, cx: &mut Context<Self>) {
        if self.asks_usage(cx) {
            self.send_command(SessionCommand::RefreshUsage);
        }
    }

    /// Whether to ask the process for usage now. An added account's asks
    /// are spaced out across chats (`Accounts::ask_usage`), as the CLI may
    /// otherwise answer with the numbers it fetched for another account.
    fn asks_usage(&self, cx: &mut Context<Self>) -> bool {
        let Some(id) = &self.account else {
            return true;
        };
        self.session.is_some()
            && self
                .accounts
                .update(cx, |accounts, _| accounts.ask_usage(id, SystemTime::now()))
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
    pub(super) fn sync_limit_timer(&mut self, cx: &mut Context<Self>) {
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
        self.refresh_usage(cx);
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
            env: self
                .account
                .as_deref()
                .map_or_else(Vec::new, |id| self.accounts.read(cx).env(id)),
            model: self.agent.chosen_model().map(str::to_owned),
            mode: self.agent.start_mode(),
            effort: self.agent.start_effort(),
            resume: self.conversation.as_ref().map(|stored| stored.id.clone()),
            fork: self.conversation.as_ref().is_some_and(|stored| stored.fork),
            tools: self.agent.tools.clone(),
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
            pid: session.pid,
            commands: session.commands,
            _driver: cx.background_spawn(session.driver),
            _events: events_task,
        })
    }

    /// Keeps the id the agent stores the conversation under. A turn's end
    /// is when the CLI has saved the conversation again.
    fn follow_session(&mut self, event: &AgentEvent, cx: &mut Context<Self>) {
        match event {
            AgentEvent::Started {
                session_id,
                cli_version,
                ..
            } => {
                self.cli.version.clone_from(cli_version);
                // A copy of a conversation keeps its name until listed.
                let title = self.conversation.take().and_then(|stored| stored.title);
                self.conversation = Some(super::Conversation {
                    id: session_id.clone(),
                    fork: false,
                    title,
                });
            }
            AgentEvent::TurnEnded(outcome) => {
                if *outcome == kiln_agent::TurnOutcome::Completed {
                    cx.emit(super::ChatEvent::TurnCompleted);
                }
            }
            _ => return,
        }
        cx.emit(super::ChatEvent::SessionChanged);
    }

    /// Keeps what the agent will not store, and writes it down once a turn
    /// or a recap ends.
    fn keep_note(&mut self, event: &AgentEvent, cx: &mut Context<Self>) {
        match event {
            AgentEvent::Started { session_id, .. } => self.kept.follow(session_id),
            AgentEvent::ConversationReset => self.kept.clear(),
            _ => self.kept.record(event),
        }
        if !matches!(event, AgentEvent::TurnEnded(_) | AgentEvent::Recap(_)) {
            return;
        }
        if let Some((path, contents)) = self.kept.take_changes() {
            cx.background_spawn(async move { write_kept(&path, &contents) })
                .detach();
        }
    }

    /// Times what a report makes expire: a usage limit, the prompt cache.
    fn start_timers(&mut self, event: &AgentEvent, cx: &mut Context<Self>) {
        match event {
            AgentEvent::Usage(_) => {
                self.remember_usage(cx);
                self.sync_limit_timer(cx);
            }
            AgentEvent::PromptTokens(_) | AgentEvent::PastPromptTokens { .. } => {
                self.sync_cache_timer(cx);
            }
            _ => {}
        }
    }

    /// A turn the agent starts by itself, such as one taking a subagent's
    /// report, is announced like one the user started.
    fn fold_into_transcript(&mut self, event: AgentEvent, cx: &mut Context<Self>) {
        let was_running = self.status() == crate::transcript::Status::Running;
        let change = self.transcript.apply(event);
        self.apply_change(change, cx);
        if !was_running && self.status() == crate::transcript::Status::Running {
            cx.emit(super::ChatEvent::TurnStarted);
        }
    }

    fn on_event(&mut self, event: AgentEvent, window: &mut Window, cx: &mut Context<Self>) {
        for command in AgentState::refreshes_after(&event) {
            if *command != SessionCommand::RefreshUsage || self.asks_usage(cx) {
                self.send_command(command.clone());
            }
        }
        let choices = (self.agent.start_mode(), self.agent.start_effort());
        if self.agent.apply(&event) {
            cx.notify();
        }
        if (self.agent.start_mode(), self.agent.start_effort()) != choices {
            cx.emit(super::ChatEvent::ChoicesChanged);
        }
        self.follow_session(&event, cx);
        self.keep_note(&event, cx);
        self.start_timers(&event, cx);
        self.update_skills(&event, cx);
        self.update_hooks(&event, cx);
        let exited = matches!(event, AgentEvent::Exited { .. });
        let turn_ended = matches!(event, AgentEvent::TurnEnded(_));
        let turn_ran = self.status() == crate::transcript::Status::Running;
        self.update_suggestion(&event, window, cx);
        if event == AgentEvent::ConversationReset {
            self.toggled.clear();
            self.scroller
                .update(cx, |scroller, cx| scroller.reset(1, cx));
        }
        self.fold_into_transcript(event, cx);
        if exited {
            self.session = None;
        }
        self.sync_prompt(window, cx);
        if turn_ended {
            self.after_turn(turn_ran, window, cx);
        }
    }

    /// What waits for a turn to end: a restart onto a newer CLI, a recap,
    /// an added account.
    fn after_turn(&mut self, turn_ran: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.cli.restart_due {
            self.restart_process(window, cx);
        }
        self.recap_due(turn_ran, window, cx);
        self.adopt_added_account(window, cx);
    }
}

fn write_kept(path: &std::path::Path, contents: &[u8]) {
    let written = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(path, contents));
    if let Err(error) = written {
        log::warn!(
            "cannot keep the chat's notes in {}: {error}",
            path.display()
        );
    }
}

/// Which CLI the chat's process runs, and whether it moves to a newer one.
#[derive(Default)]
pub(super) struct CliRun {
    version: Option<String>,
    /// Asked to move while a turn ran; it moves once the turn ends.
    restart_due: bool,
}

impl ChatView {
    /// The version of the CLI the chat's process runs, once it said.
    #[must_use]
    pub fn cli_version(&self) -> Option<&str> {
        self.session.as_ref().and(self.cli.version.as_deref())
    }

    /// Moves the conversation onto the CLI installed now: the process
    /// restarts and resumes it, at once or when the turn under way ends.
    pub fn restart_on_new_cli(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.session.is_none() {
            return;
        }
        if self.status() == crate::transcript::Status::Running {
            self.cli.restart_due = true;
            return;
        }
        self.restart_process(window, cx);
    }

    fn restart_process(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cli.restart_due = false;
        self.session = None;
        let change = self
            .transcript
            .push_notice("Restarted on the updated CLI".to_owned());
        self.apply_change(change, cx);
        self.ensure_session(window, cx);
        cx.notify();
    }
}
