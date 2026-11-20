//! The chat panel's state: one agent session, its transcript, the open
//! permission prompt and the composer. Layout lives in `layout`.

mod composer;
mod header;
mod layout;
mod menu_keys;
mod native;
mod session;
mod slash_menu;

use std::{collections::HashSet, path::PathBuf, rc::Rc};

use gpui_kit::prelude::*;
use gpui_kit::{
    Context, Entity, FocusHandle, Focusable, Subscription, Task, Window,
    component::{
        input::{InputEvent, TextareaState},
        message_scroller::MessageScrollerState,
    },
};
use kiln_agent::{AgentBackend, SessionCommand};

use crate::{
    agent_state::AgentState,
    permission_prompt::{Answered, PermissionPrompt},
    slash, spinner,
    transcript::{Change, Status, Transcript},
};
use native::ModelPicker;
use slash_menu::SlashMenu;

const COMPOSER_MAX_ROWS: usize = 10;

/// A running agent process. Dropping it stops the process.
struct LiveSession {
    commands: async_channel::Sender<SessionCommand>,
    _driver: Task<()>,
    _events: Task<()>,
}

pub struct ChatView {
    backend: Rc<dyn AgentBackend>,
    cwd: PathBuf,
    transcript: Transcript,
    /// Entries the user toggled away from their default expansion.
    toggled: HashSet<u64>,
    session: Option<LiveSession>,
    agent: AgentState,
    /// Transcript rows plus one trailing row for the spinner.
    scroller: Entity<MessageScrollerState>,
    composer: Entity<TextareaState>,
    slash: SlashMenu,
    model_picker: Option<ModelPicker>,
    usage_open: bool,
    prompt: Option<(Entity<PermissionPrompt>, Subscription)>,
    spinner_frame: usize,
    spinner: Option<Task<()>>,
    /// Lifts a usage limit when its reset time comes.
    limit_timer: Option<Task<()>>,
    _usage_poll: Task<()>,
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
        let subscription =
            cx.subscribe_in(
                &composer,
                window,
                |this, _, event, window, cx| match event {
                    InputEvent::PressEnter { shift: false, .. } => this.send(window, cx),
                    InputEvent::Change => {
                        // Typing a message puts the picker away; the text box
                        // emptying as a command runs leaves it open.
                        if !this.composer.read(cx).value().is_empty() {
                            this.close_model_picker(cx);
                        }
                        this.sync_slash_menu(cx);
                    }
                    _ => {}
                },
            );
        let mut view = Self {
            backend,
            cwd,
            transcript: Transcript::default(),
            toggled: HashSet::new(),
            session: None,
            agent: AgentState::default(),
            scroller: cx.new(|cx| MessageScrollerState::new(1, cx)),
            composer,
            slash: SlashMenu::default(),
            model_picker: None,
            usage_open: false,
            prompt: None,
            spinner_frame: 0,
            spinner: None,
            limit_timer: None,
            _usage_poll: cx.spawn(Self::poll_usage),
            _subscriptions: vec![subscription],
        };
        // Started now, not on the first message: the composer shows the
        // models, mode and context the agent reports when it starts.
        view.ensure_session(window, cx);
        view
    }

    #[must_use]
    pub fn status(&self) -> Status {
        self.transcript.status()
    }

    /// Sends the composer's text, or runs it when it is a command Kiln
    /// answers itself.
    pub fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).value().trim().to_owned();
        if let Some(command) = slash::native(&text) {
            self.composer
                .update(cx, |composer, cx| composer.clean(window, cx));
            self.run_native(command, window, cx);
            return;
        }
        if self.submit(text, window, cx) {
            self.composer
                .update(cx, |composer, cx| composer.clean(window, cx));
        }
    }

    /// Sends `text` as the next message; `false` if it cannot be sent now
    /// (empty, a turn is running, a usage limit is reached, or the agent
    /// failed to start).
    pub fn submit(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if text.is_empty() || self.status() == Status::Running || self.is_limited() {
            return false;
        }
        let Some(commands) = self.ensure_session(window, cx) else {
            return false;
        };
        let change = self.transcript.push_user(text.clone());
        self.apply_change(change, cx);
        commands.try_send(SessionCommand::Send(text)).is_ok()
    }

    pub fn interrupt(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.send_command(SessionCommand::Interrupt);
        cx.notify();
    }

    /// Ends the session and starts over with an empty transcript.
    pub fn new_conversation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.session = None;
        self.transcript = Transcript::default();
        self.toggled.clear();
        self.agent.reset_context();
        self.scroller
            .update(cx, |scroller, cx| scroller.reset(1, cx));
        self.sync_prompt(window, cx);
        self.sync_spinner(cx);
        self.ensure_session(window, cx);
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
        self.send_command(SessionCommand::Permission {
            request_id: request.id,
            decision: answered.decision.clone(),
        });
        self.sync_prompt(window, cx);
        cx.notify();
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
