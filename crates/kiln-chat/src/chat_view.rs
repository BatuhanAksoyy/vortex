//! The chat panel's state: one agent session, its transcript, the open
//! permission prompt and the composer. Layout lives in `layout`.

mod accounts;
mod accounts_panel;
mod composer;
mod context_panel;
mod header;
mod history;
mod layout;
mod menu_keys;
mod native;
mod session;
mod skills;
mod skills_panel;
mod slash_menu;

use std::{collections::HashSet, path::PathBuf, rc::Rc};

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyView, Context, Entity, EventEmitter, FocusHandle, Focusable, Subscription, Task, Window,
    component::{
        input::{InputEvent, TextareaState},
        message_scroller::MessageScrollerState,
    },
};
use kiln_accounts::Accounts;
use kiln_agent::{AgentBackend, SessionCommand};

use crate::{
    agent_state::AgentState,
    permission_prompt::{Answered, PermissionPrompt},
    question_prompt::QuestionPrompt,
    skill_menu::SkillMenu,
    slash, spinner,
    transcript::{Change, Status, Transcript},
};
use accounts::AccountsPanel;
pub use history::ChatEvent;
use history::Conversation;
use native::ModelPicker;
use slash_menu::SlashMenu;

const COMPOSER_MAX_ROWS: usize = 10;

/// A running agent process. Dropping it stops the process.
struct LiveSession {
    commands: async_channel::Sender<SessionCommand>,
    _driver: Task<()>,
    _events: Task<()>,
}

/// The prompt shown in place of the composer while a request waits: a
/// permission question, or the agent's own questions.
struct ShownPrompt {
    request_id: String,
    view: AnyView,
    focus: FocusHandle,
    _answered: Subscription,
}

pub struct ChatView {
    backend: Rc<dyn AgentBackend>,
    /// Shared by every chat; added accounts show in all of them.
    accounts: Entity<Accounts>,
    /// The added account the chat runs with, by id; `None` is the user's
    /// usual sign-in, used only while no account is added.
    account: Option<String>,
    accounts_panel: AccountsPanel,
    cwd: PathBuf,
    transcript: Transcript,
    /// Entries the user toggled away from their default expansion.
    toggled: HashSet<u64>,
    session: Option<LiveSession>,
    /// The stored conversation shown; a process started after the last one
    /// ended resumes it.
    conversation: Option<Conversation>,
    /// Reads a reopened conversation's messages.
    history_load: Option<Task<()>>,
    agent: AgentState,
    /// Transcript rows plus one trailing row for the spinner.
    scroller: Entity<MessageScrollerState>,
    composer: Entity<TextareaState>,
    slash: SlashMenu,
    model_picker: Option<ModelPicker>,
    usage_open: bool,
    context_open: bool,
    prompt: Option<ShownPrompt>,
    skill_menu: SkillMenu,
    skills_open: bool,
    /// Reads the skills again until a saved state shows up.
    skill_check: Option<Task<()>>,
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
        accounts: Entity<Accounts>,
        cwd: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let composer = new_composer(window, cx);
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
        let accounts_changed = cx.observe_in(&accounts, window, |this, _, window, cx| {
            this.adopt_added_account(window, cx);
            cx.notify();
        });
        let account = accounts
            .read(cx)
            .added()
            .first()
            .map(|account| account.id.clone());
        let mut view = Self {
            backend,
            accounts,
            account,
            accounts_panel: AccountsPanel::default(),
            cwd,
            transcript: Transcript::default(),
            toggled: HashSet::new(),
            session: None,
            conversation: None,
            history_load: None,
            agent: AgentState::default(),
            scroller: cx.new(|cx| MessageScrollerState::new(1, cx)),
            composer,
            slash: SlashMenu::default(),
            model_picker: None,
            usage_open: false,
            context_open: false,
            prompt: None,
            skill_menu: SkillMenu::default(),
            skills_open: false,
            skill_check: None,
            spinner_frame: 0,
            spinner: None,
            limit_timer: None,
            _usage_poll: cx.spawn(Self::poll_usage),
            _subscriptions: vec![subscription, accounts_changed],
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
        cx.emit(ChatEvent::TurnStarted);
        commands.try_send(SessionCommand::Send(text)).is_ok()
    }

    pub fn interrupt(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.send_command(SessionCommand::Interrupt);
        cx.notify();
    }

    /// Ends the session and starts over with an empty transcript.
    pub fn new_conversation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.session = None;
        self.conversation = None;
        self.history_load = None;
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
        cx.emit(ChatEvent::SessionChanged);
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
            .map(|prompt| prompt.request_id.as_str());
        if pending.map(|request| request.id.as_str()) == shown {
            return;
        }
        let had_focus = self.chat_focused(window, cx);
        self.prompt = pending.cloned().map(|request| {
            let request_id = request.id.clone();
            if request.questions.is_empty() {
                let prompt = cx.new(|cx| PermissionPrompt::new(request, &self.cwd, window, cx));
                Self::show_prompt(request_id, prompt, window, cx)
            } else {
                let prompt = cx.new(|cx| QuestionPrompt::new(&request, window, cx));
                Self::show_prompt(request_id, prompt, window, cx)
            }
        });
        if had_focus {
            let focus = self.focus_handle(cx);
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    fn show_prompt<P: Render + Focusable + EventEmitter<Answered>>(
        request_id: String,
        prompt: Entity<P>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ShownPrompt {
        let answered = cx.subscribe_in(&prompt, window, |this, _, answered, window, cx| {
            this.answer(answered, window, cx);
        });
        ShownPrompt {
            request_id,
            focus: prompt.focus_handle(cx),
            view: prompt.into(),
            _answered: answered,
        }
    }

    fn chat_focused(&self, window: &Window, cx: &gpui_kit::App) -> bool {
        self.composer.read(cx).focus_handle(cx).is_focused(window)
            || self
                .prompt
                .as_ref()
                .is_some_and(|prompt| prompt.focus.contains_focused(window, cx))
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

fn new_composer(window: &mut Window, cx: &mut Context<ChatView>) -> Entity<TextareaState> {
    cx.new(|cx| {
        let mut state = TextareaState::new(window, cx)
            .auto_grow(1, COMPOSER_MAX_ROWS)
            .placeholder("Ask Claude to edit, explain or run something…");
        state.set_submit_on_enter(true, cx);
        state
    })
}

impl Focusable for ChatView {
    fn focus_handle(&self, cx: &gpui_kit::App) -> FocusHandle {
        match &self.prompt {
            Some(prompt) => prompt.focus.clone(),
            None => self.composer.read(cx).focus_handle(cx),
        }
    }
}
