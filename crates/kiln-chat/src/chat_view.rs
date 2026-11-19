//! The chat panel's state: one agent session, its transcript, the open
//! permission prompt and the composer. Layout lives in `layout`.

mod layout;

use std::{collections::HashSet, path::PathBuf, rc::Rc};

use gpui_kit::prelude::*;
use gpui_kit::{
    Context, Entity, FocusHandle, Focusable, Subscription, Task, Window,
    component::{
        input::{InputEvent, TextareaState},
        message_scroller::MessageScrollerState,
    },
};
use kiln_agent::{AgentBackend, AgentEvent, Decision, Remember, SessionCommand, SessionSpec};

use crate::{
    permission_prompt::{Answered, PermissionPrompt},
    spinner,
    transcript::{Change, Status, Transcript},
};

const COMPOSER_MAX_ROWS: usize = 10;

/// A running agent process. Dropping it stops the process.
struct LiveSession {
    commands: async_channel::Sender<SessionCommand>,
    _driver: Task<()>,
    _events: Task<()>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    AskBeforeEdits,
    EditAutomatically,
}

pub struct ChatView {
    backend: Rc<dyn AgentBackend>,
    cwd: PathBuf,
    transcript: Transcript,
    /// Entries the user toggled away from their default expansion.
    toggled: HashSet<u64>,
    session: Option<LiveSession>,
    mode: Mode,
    /// Transcript rows plus one trailing row for the spinner.
    scroller: Entity<MessageScrollerState>,
    composer: Entity<TextareaState>,
    prompt: Option<(Entity<PermissionPrompt>, Subscription)>,
    spinner_frame: usize,
    spinner: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl ChatView {
    pub fn new(
        backend: Rc<dyn AgentBackend>,
        cwd: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let composer = cx.new(|cx| {
            let mut state = TextareaState::new(window, cx)
                .auto_grow(1, COMPOSER_MAX_ROWS)
                .placeholder("Ask Claude to edit, explain or run something…");
            state.set_submit_on_enter(true, cx);
            state
        });
        let subscription = cx.subscribe_in(&composer, window, |this, _, event, window, cx| {
            if let InputEvent::PressEnter { shift: false, .. } = event {
                this.send(window, cx);
            }
        });
        Self {
            backend,
            cwd,
            transcript: Transcript::default(),
            toggled: HashSet::new(),
            session: None,
            mode: Mode::AskBeforeEdits,
            scroller: cx.new(|cx| MessageScrollerState::new(1, cx)),
            composer,
            prompt: None,
            spinner_frame: 0,
            spinner: None,
            _subscriptions: vec![subscription],
        }
    }

    #[must_use]
    pub fn status(&self) -> Status {
        self.transcript.status()
    }

    /// Sends the composer's text.
    pub fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).value().trim().to_owned();
        if self.submit(text, window, cx) {
            self.composer
                .update(cx, |composer, cx| composer.clean(window, cx));
        }
    }

    /// Sends `text` as the next message; `false` if it cannot be sent now
    /// (empty, a turn is running, or the agent failed to start).
    pub fn submit(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if text.is_empty() || self.status() == Status::Running {
            return false;
        }
        let Some(commands) = self.live_commands(window, cx) else {
            return false;
        };
        let change = self.transcript.push_user(text.clone());
        self.apply_change(change, cx);
        commands.try_send(SessionCommand::Send(text)).is_ok()
    }

    pub fn interrupt(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = &self.session {
            let _ = session.commands.try_send(SessionCommand::Interrupt);
        }
        cx.notify();
    }

    /// Ends the session and starts over with an empty transcript.
    pub fn new_conversation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.session = None;
        self.transcript = Transcript::default();
        self.toggled.clear();
        self.mode = Mode::AskBeforeEdits;
        self.scroller
            .update(cx, |scroller, cx| scroller.reset(1, cx));
        self.sync_prompt(window, cx);
        self.sync_spinner(cx);
        let focus = self.composer.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        cx.notify();
    }

    pub fn toggle_expanded(&mut self, id: u64, cx: &mut Context<Self>) {
        if !self.toggled.remove(&id) {
            self.toggled.insert(id);
        }
        self.scroller.update(cx, MessageScrollerState::remeasure);
    }

    fn answer(&mut self, answered: &Answered, window: &mut Window, cx: &mut Context<Self>) {
        let Some(request) = self.transcript.take_permission(&answered.request_id) else {
            return;
        };
        if answered.decision == Decision::AllowAlways
            && request.remember == Some(Remember::AcceptEdits)
        {
            self.mode = Mode::EditAutomatically;
        }
        if let Some(session) = &self.session {
            let _ = session.commands.try_send(SessionCommand::Permission {
                request_id: request.id,
                decision: answered.decision.clone(),
            });
        }
        self.sync_prompt(window, cx);
        cx.notify();
    }

    /// The running session's command channel, starting a session if needed.
    fn live_commands(
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
        let exited = matches!(event, AgentEvent::Exited { .. });
        let change = self.transcript.apply(event);
        self.apply_change(change, cx);
        if exited {
            self.session = None;
        }
        self.sync_prompt(window, cx);
    }

    /// Shows the oldest unanswered permission request in place of the
    /// composer. Focus follows only if the chat had it: a prompt must never
    /// grab keys typed elsewhere, where a stray `1` would approve a tool call.
    fn sync_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pending = self.transcript.pending_permission();
        let shown = self
            .prompt
            .as_ref()
            .map(|(prompt, _)| prompt.read(cx).request_id().to_owned());
        if pending.map(|request| request.id.as_str()) == shown.as_deref() {
            return;
        }
        let had_focus = self.chat_focused(window, cx);
        self.prompt = pending.cloned().map(|request| {
            let prompt = cx.new(|cx| PermissionPrompt::new(request, &self.cwd, window, cx));
            let subscription = cx.subscribe_in(&prompt, window, |this, _, answered, window, cx| {
                this.answer(answered, window, cx);
            });
            (prompt, subscription)
        });
        if had_focus {
            let focus = match &self.prompt {
                Some((prompt, _)) => prompt.focus_handle(cx),
                None => self.composer.read(cx).focus_handle(cx),
            };
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    fn chat_focused(&self, window: &Window, cx: &gpui_kit::App) -> bool {
        self.composer.read(cx).focus_handle(cx).is_focused(window)
            || self
                .prompt
                .as_ref()
                .is_some_and(|(prompt, _)| prompt.focus_handle(cx).contains_focused(window, cx))
    }

    fn apply_change(&mut self, change: Change, cx: &mut Context<Self>) {
        let len = self.transcript.len();
        self.scroller.update(cx, |scroller, cx| {
            if let Some(index) = change.updated {
                scroller.remeasure_items(index..index + 1, cx);
            }
            if change.appended > 0 {
                let tail = len - change.appended;
                scroller.splice(tail..tail, change.appended, cx);
            }
        });
        self.sync_spinner(cx);
        cx.notify();
    }

    fn next_spinner_frame(&mut self, cx: &mut Context<Self>) {
        self.spinner_frame += 1;
        cx.notify();
    }

    /// Runs the spinner's frame timer while a turn is in progress.
    fn sync_spinner(&mut self, cx: &mut Context<Self>) {
        let running = self.status() == Status::Running;
        if running == self.spinner.is_some() {
            return;
        }
        let tail = self.transcript.len();
        self.scroller.update(cx, |scroller, cx| {
            scroller.remeasure_items(tail..tail + 1, cx)
        });
        self.spinner = running.then(|| cx.spawn(Self::animate_spinner));
    }

    async fn animate_spinner(this: gpui_kit::WeakEntity<Self>, cx: &mut gpui_kit::AsyncApp) {
        let mut alive = true;
        while alive {
            cx.background_executor().timer(spinner::FRAME).await;
            alive = this.update(cx, Self::next_spinner_frame).is_ok();
        }
    }
}

impl Focusable for ChatView {
    fn focus_handle(&self, cx: &gpui_kit::App) -> FocusHandle {
        match &self.prompt {
            Some((prompt, _)) => prompt.focus_handle(cx),
            None => self.composer.read(cx).focus_handle(cx),
        }
    }
}
