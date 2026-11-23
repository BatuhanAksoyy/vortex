//! Agent-neutral session model. Each backend (`kiln-agent-claude`, later
//! `kiln-agent-codex`) translates its own protocol into these types, so the UI
//! never sees protocol details.

mod hooks;
mod past;
mod permission;

pub use hooks::{Hook, HookEdit, HookEvent, HookScope, Hooks, StoredHook};
pub use past::{PastMessage, PastSession};
pub use permission::{Decision, PermissionRequest, Question, QuestionOption, Remember, RuleScope};

use std::{
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    time::SystemTime,
};

use serde_json::Value;

/// Where and how to start a session.
#[derive(Clone, Debug)]
pub struct SessionSpec {
    pub cwd: PathBuf,
    /// Extra environment, e.g. the account's config directory.
    pub env: Vec<(String, String)>,
    /// Model id or alias to start with; `None` keeps the agent's default.
    pub model: Option<String>,
    /// Permission mode to start in; `None` keeps the agent's default.
    pub mode: Option<PermissionMode>,
    /// A session to continue, by the id `AgentEvent::Started` reported;
    /// `None` starts a new conversation.
    pub resume: Option<String>,
    /// Continues `resume` as a new conversation of its own, leaving the
    /// stored one as it was.
    pub fork: bool,
}

pub trait AgentBackend {
    fn spawn(&self, spec: SessionSpec) -> anyhow::Result<Session>;

    /// Signs an account in to the credential store `env` points at. The user
    /// finishes in the browser, on the page `SignIn::url` hands out.
    fn sign_in(&self, env: Vec<(String, String)>) -> anyhow::Result<SignIn>;

    /// The conversations the agent stored for `cwd`, most recently active
    /// first. Reads files; poll it on a background executor.
    fn past_sessions(&self, cwd: &Path) -> Pending<Vec<PastSession>>;

    /// The messages of stored conversation `session_id`, oldest first, as a
    /// reopened chat shows them.
    fn past_messages(&self, cwd: &Path, session_id: &str) -> Pending<Vec<PastMessage>>;
}

/// Work a backend hands out to be polled, typically on a background executor.
pub type Pending<T> = Pin<Box<dyn Future<Output = anyhow::Result<T>> + Send>>;

/// A sign-in in progress. Dropping `done` cancels it.
pub struct SignIn {
    /// The page to sign in on, sent once the agent has it.
    pub url: async_channel::Receiver<String>,
    /// Ends with the account signed in; the owner must poll it.
    pub done: Pending<Account>,
}

/// A running session. The owner must poll `driver` (typically on a
/// background executor); dropping `commands` ends the session.
pub struct Session {
    pub events: async_channel::Receiver<AgentEvent>,
    pub commands: async_channel::Sender<SessionCommand>,
    pub driver: Pin<Box<dyn Future<Output = ()> + Send>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionCommand {
    Send(String),
    Permission {
        request_id: String,
        decision: Decision,
    },
    Interrupt,
    /// Switches the model for the following turns.
    SetModel(String),
    SetMode(PermissionMode),
    /// Asks for a fresh [`AgentEvent::ContextUsage`].
    RefreshContext,
    /// Asks for a fresh [`AgentEvent::Usage`].
    RefreshUsage,
    /// Asks for a fresh [`AgentEvent::Skills`].
    RefreshSkills,
    /// Saves how the skill named `name` is offered; a fresh
    /// [`AgentEvent::Skills`] or an [`AgentEvent::SkillStateFailed`] follows.
    SetSkillState {
        name: String,
        state: SkillState,
    },
    /// Asks for a fresh [`AgentEvent::Hooks`].
    RefreshHooks,
    /// Adds or removes a hook in its settings file; an
    /// [`AgentEvent::HookEdited`] follows.
    EditHook(HookEdit),
}

/// How much the agent may do without asking.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    /// Asks before edits and most commands.
    #[default]
    AskBeforeEdits,
    EditAutomatically,
    /// Plans without changing anything until the plan is approved.
    Plan,
    /// A classifier approves most actions instead of asking.
    Auto,
    /// Denies whatever is not pre-approved instead of asking.
    DontAsk,
    BypassPermissions,
}

/// A model the agent offers, as it names and describes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelOption {
    /// What to pass to [`SessionCommand::SetModel`], e.g. `sonnet`.
    pub id: String,
    /// The concrete model the id currently means.
    pub resolved: String,
    pub name: String,
    pub description: String,
}

/// What the agent reports once it is ready, before any turn.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionInfo {
    pub models: Vec<ModelOption>,
    pub mode: PermissionMode,
    pub account: Option<Account>,
    pub commands: Vec<SlashCommand>,
}

/// A command the agent runs itself when a message starts with `/name`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SlashCommand {
    pub name: String,
    pub description: String,
    /// What may follow the name, e.g. `<optional instructions>`; empty when
    /// the command takes nothing.
    pub argument_hint: String,
    /// Other names that run the same command.
    pub aliases: Vec<String>,
}

/// The account the agent is signed in with. Fields the agent leaves out are
/// empty.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Account {
    pub email: String,
    pub organization: String,
    /// The subscription, e.g. "Claude Pro".
    pub plan: String,
}

/// How much of the subscription's usage limits is spent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Usage {
    /// The short window (five hours for Claude).
    pub session: Option<UsageWindow>,
    pub weekly: Option<UsageWindow>,
    /// Set while a limit is reached and messages would be refused.
    pub limited: Option<Limited>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct UsageWindow {
    /// Whole percent used, 0–100.
    pub percent: u8,
    pub resets_at: Option<SystemTime>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Limited {
    /// When sending works again, if the agent says.
    pub until: Option<SystemTime>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ContextUsage {
    pub used_tokens: u64,
    pub max_tokens: u64,
    /// Whole percent in use as the agent itself shows it, when it says.
    pub percent: Option<u64>,
    /// Where the context goes, as the agent breaks it down; empty when it
    /// does not.
    pub categories: Vec<ContextCategory>,
}

/// One part of the context, e.g. the system prompt or the messages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextCategory {
    pub name: String,
    pub tokens: u64,
    pub kind: ContextKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextKind {
    /// Counted in [`ContextUsage::used_tokens`].
    Used,
    /// Loaded only when needed, so not counted as used.
    Deferred,
    /// Kept free so the conversation can be compacted.
    Buffer,
    Free,
}

/// A skill the agent can load, as its skill settings list it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skill {
    /// The name its settings are saved under, e.g. `plugin:skill`.
    pub name: String,
    /// The name the user types after `/`.
    pub display_name: String,
    pub description: String,
    /// Where it comes from, e.g. `project`, `user` or `plugin`.
    pub source: String,
    /// What listing it costs in the system prompt.
    pub tokens: u64,
    pub state: SkillState,
    /// Set when the state cannot be changed here.
    pub locked_by: Option<SkillLock>,
    /// Whether it is offered as a slash command; a skill that is off is not.
    pub advertised: bool,
}

/// How a skill is offered, from most to least visible.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SkillState {
    /// Listed for the model, and the user can run it.
    #[default]
    On,
    /// Listed for the model by name only, without its description.
    NameOnly,
    /// The user can run it; the model does not see it.
    UserOnly,
    /// Hidden from the model and from the command list.
    Off,
}

impl SkillState {
    pub const ALL: [Self; 4] = [Self::On, Self::NameOnly, Self::UserOnly, Self::Off];

    /// The state after this one, wrapping around.
    #[must_use]
    pub fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|state| *state == self)
            .unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }
}

/// Why a skill's state cannot be changed here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkillLock {
    /// The skill's own file sets it.
    Author,
    /// Its plugin manages it.
    Plugin,
    /// Managed or start-up settings set it.
    Policy,
    /// Settings cannot hold an entry under its name.
    ReservedName,
}

/// Identifies one streamed content block, so consecutive deltas can be
/// appended to the same transcript entry.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BlockKey {
    pub message_id: String,
    pub index: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AgentEvent {
    Started {
        session_id: String,
        model: String,
    },
    TextDelta {
        block: BlockKey,
        text: String,
    },
    ThinkingDelta {
        block: BlockKey,
        text: String,
    },
    ToolCall(ToolCall),
    ToolResult {
        id: String,
        output: String,
        is_error: bool,
    },
    PermissionRequest(PermissionRequest),
    Ready(SessionInfo),
    /// The model in use changed; carries the concrete model id.
    ModelChanged(String),
    ModeChanged(PermissionMode),
    ContextUsage(ContextUsage),
    /// Usage limits, both when asked for and as the agent reports them
    /// during a turn.
    Usage(Usage),
    /// A model or mode change was refused; the message says why.
    CommandFailed(String),
    /// The slash commands on offer changed, e.g. after skills were loaded.
    CommandsChanged(Vec<SlashCommand>),
    /// The skills the agent can load, with how each is offered.
    Skills(Vec<Skill>),
    /// Saving a skill's state failed; the message says why.
    SkillStateFailed {
        name: String,
        error: String,
    },
    /// The hooks configured for the session, from every source.
    Hooks(Hooks),
    /// A hook edit was saved, or failed with the message given. The agent
    /// may list the change only a moment later.
    HookEdited {
        error: Option<String>,
    },
    /// The agent started over with an empty conversation, e.g. on `/clear`.
    ConversationReset,
    TurnEnded(TurnOutcome),
    /// The agent process is gone; no further events follow.
    Exited {
        error: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ToolCall {
    pub id: String,
    /// The tool call of the subagent that made this call, if any.
    pub parent: Option<String>,
    pub name: String,
    pub input: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TurnOutcome {
    Completed,
    Interrupted,
    Failed(String),
}
