//! The chat panel's state: one agent session, its transcript, the open
//! permission prompt and the composer. Layout lives in `layout`.

mod accounts;
mod accounts_panel;
mod agents_dialog;
mod agents_panel;
pub mod asking;
pub(crate) mod attachments;
mod bypass;
mod cache_panel;
mod composer;
mod composer_box;
mod context_panel;
mod find;
mod header;
mod history;
mod hook_form;
mod hooks;
mod hooks_panel;
mod layout;
mod menu_keys;
mod native;
mod picture_viewer;
mod processes_page;
mod recap;
mod session;
mod skills;
mod skills_panel;
mod slash_menu;
mod sleep;
mod tokens_panel;
mod turn_keys;

use hooks::HooksPanel;

use std::{cell::Cell, collections::HashSet, path::PathBuf, rc::Rc};

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyView, Context, Entity, EventEmitter, FocusHandle, Focusable, Pixels, Subscription, Task,
    Window,
    component::{input::TextareaState, message_scroller::MessageScrollerState},
};
use kiln_accounts::Accounts;
use kiln_agent::{AgentBackend, SessionCommand};

use crate::{
    agent_state::AgentState,
    permission_prompt::{Answered, PermissionPrompt},
    question_prompt::QuestionPrompt,
    skill_menu::SkillMenu,
    spinner,
    transcript::{Change, Status, Transcript},
};
use accounts::AccountsPanel;
pub use history::ChatEvent;
use history::Conversation;
use native::ModelPicker;
use slash_menu::SlashMenu;

/// A running agent process. Dropping it stops the process.
struct LiveSession {
    pid: Option<u32>,
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

/// The panels the header and composer strip open; one is open at a time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StripPanel {
    Tokens,
    Usage,
    Context,
    Cache,
    Skills,
    Hooks,
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
    /// What the agent's processes use; `None` when the app reads none.
    process_meter: Option<Entity<crate::processes::ProcessMeter>>,
    processes_page: Option<(Entity<crate::processes::ProcessesDialog>, Subscription)>,
    /// The stored conversation shown; a process started after the last one
    /// ended resumes it.
    conversation: Option<Conversation>,
    /// Reads a reopened conversation's messages.
    history_load: Option<Task<()>>,
    agent: AgentState,
    /// Transcript rows plus one trailing row for the spinner.
    scroller: Entity<MessageScrollerState>,
    /// The transcript's width last frame, and how many times it changed.
    transcript_width: Rc<Cell<(Option<Pixels>, u32)>>,
    composer: Entity<TextareaState>,
    /// The message the agent predicts the user sends next, offered in the
    /// empty composer.
    suggestion: Option<String>,
    recap: recap::RecapWatch,
    kept: crate::kept_notes::KeptNotes,
    cli: session::CliRun,
    sleep: sleep::Sleep,
    find: Option<find::Find>,
    /// The sent message Up last put in the box, by its place from the
    /// newest, while the box still holds it unchanged.
    recalled: Option<(usize, String)>,
    slash: SlashMenu,
    model_picker: Option<ModelPicker>,
    open_panel: Option<StripPanel>,
    prompt: Option<ShownPrompt>,
    skill_menu: SkillMenu,
    /// Reads the skills again until a saved state shows up.
    skill_check: Option<Task<()>>,
    hooks: HooksPanel,
    /// A picture shown at full size over the chat.
    viewer: picture_viewer::PictureViewer,
    /// Pictures and files to send with the next message.
    attachments: Vec<attachments::Attachment>,
    /// The subagents view, open over this chat only.
    agents: Option<Entity<agents_dialog::AgentsDialog>>,
    spinner_frame: usize,
    spinner: Option<Task<()>>,
    /// Lifts a usage limit when its reset time comes.
    limit_timer: Option<Task<()>>,
    /// A usage limit cut the last turn off; another account carries it on.
    cut_by_limit: bool,
    /// Redraws the cache button when the prompt cache expires.
    cache_timer: Option<Task<()>>,
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
        let composer = composer_box::new_composer(window, cx);
        let subscription = Self::follow_composer(&composer, window, cx);
        let accounts_changed = Self::follow_accounts(&accounts, window, cx);
        let activation = cx.observe_window_activation(window, Self::watch_for_recap);
        let process_meter = crate::processes::ProcessMeter::for_chat(cx.entity().downgrade(), cx);
        let meter_opened = process_meter
            .as_ref()
            .map(|meter| Self::follow_meter(meter, window, cx));
        let account = accounts.read(cx).added().first().map(|it| it.id.clone());
        let mut view = Self {
            backend,
            accounts,
            account,
            accounts_panel: AccountsPanel::default(),
            cwd,
            transcript: Transcript::default(),
            toggled: HashSet::new(),
            session: None,
            process_meter,
            processes_page: None,
            conversation: None,
            history_load: None,
            agent: AgentState::default(),
            scroller: cx.new(|cx| MessageScrollerState::new(1, cx)),
            transcript_width: Rc::default(),
            composer,
            suggestion: None,
            recap: recap::RecapWatch::default(),
            kept: crate::kept_notes::KeptNotes::default(),
            cli: session::CliRun::default(),
            sleep: sleep::Sleep::default(),
            find: None,
            recalled: None,
            slash: SlashMenu::default(),
            model_picker: None,
            open_panel: None,
            prompt: None,
            skill_menu: SkillMenu::default(),
            skill_check: None,
            hooks: HooksPanel::new(window, cx),
            agents: None,
            attachments: Vec::new(),
            viewer: picture_viewer::PictureViewer::new(cx),
            spinner_frame: 0,
            spinner: None,
            limit_timer: None,
            cut_by_limit: false,
            cache_timer: None,
            _usage_poll: cx.spawn(Self::poll_usage),
            _subscriptions: [subscription, accounts_changed, activation]
                .into_iter()
                .chain(meter_opened)
                .collect(),
        };
        // Started now, not on the first message: the composer shows the
        // models, mode and context the agent reports when it starts.
        // Deferred, so a chat created to reopen a conversation starts only
        // the process that resumes it.
        cx.defer_in(window, |this, window, cx| {
            this.ensure_session(window, cx);
        });
        view.recall_usage(cx);
        view
    }

    fn follow_accounts(
        accounts: &Entity<Accounts>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Subscription {
        cx.observe_in(accounts, window, |this, _, window, cx| {
            this.adopt_added_account(window, cx);
            // Another chat with the same account may have learned newer
            // usage.
            this.recall_usage(cx);
            cx.notify();
        })
    }

    #[must_use]
    pub fn status(&self) -> Status {
        self.transcript.status()
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
        self.find = None;
        self.agent.reset_context();
        self.drop_suggestion(window, cx);
        self.recap = recap::RecapWatch::default();
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
        self.refresh_find(false, cx);
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
        self.refresh_find(false, cx);
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
            Some(prompt) => prompt.focus.clone(),
            None => self.composer.read(cx).focus_handle(cx),
        }
    }
}
