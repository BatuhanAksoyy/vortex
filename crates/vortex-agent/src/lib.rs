//! Agent-neutral session model. Each backend (`vortex-agent-claude`,
//! `vortex-agent-codex`) translates its own protocol into these types, so the UI
//! never sees protocol details.

mod effort;
mod features;
mod hooks;
mod install;
mod message;
mod one_shot;
mod past;
mod permission;
mod setup;
mod skills;
mod subagent;
mod tools;
mod usage;

pub use effort::{Effort, PromptTokens, SpentTokens};
pub use features::Features;
pub use hooks::{Hook, HookEdit, HookEvent, HookScope, Hooks, StoredHook};
pub use install::{Cli, Install, Method, fetch, is_newer, parse_version};
pub use message::{Message, Picture};
pub use one_shot::OneShot;
pub use past::{PastMessage, PastSession};
pub use permission::{Decision, PermissionRequest, Question, QuestionOption, Remember, RuleScope};
pub use setup::{install, install_command, is_installed, on_path};
pub use skills::{Skill, SkillLock, SkillState};
pub use subagent::{SubagentChange, SubagentEnd, SubagentEvent, SubagentUsage};
pub use tools::ToolServer;
pub use usage::{ContextCategory, ContextKind, ContextUsage, Limited, Usage, UsageWindow};

use std::{
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    time::{Duration, SystemTime},
};

use serde_json::Value;

/// Set by a parent Claude Code process. Inherited, they make a spawned CLI
/// behave as a nested session of whatever terminal launched Vortex, so no
/// backend passes them on.
pub const PARENT_SESSION_VARS: &[&str] = &[
    "CLAUDECODE",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_CODE_EXECPATH",
    "CLAUDE_PID",
    "CLAUDE_EFFORT",
    "AI_AGENT",
];

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
    /// Effort to start with; `None` keeps the agent's default.
    pub effort: Option<Effort>,
    /// A session to continue, by the id `AgentEvent::Started` reported;
    /// `None` starts a new conversation.
    pub resume: Option<String>,
    /// Continues `resume` as a new conversation of its own, leaving the
    /// stored one as it was.
    pub fork: bool,
    /// Vortex's tools; the agent uses them without asking, Vortex asks instead.
    pub tools: Option<ToolServer>,
    /// The conversation so far, as another harness held it, for a new
    /// conversation that continues it; given to the agent as instructions.
    pub handoff: Option<String>,
}

pub trait AgentBackend {
    /// The harness's name in the settings, such as `claude`.
    fn name(&self) -> &'static str;

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

    /// What of Vortex's session features the harness has; a chat hides the
    /// rest rather than show them empty.
    fn features(&self) -> Features {
        Features::ALL
    }

    /// One answer to `ask.prompt`, from a model run for it alone, with no
    /// tools; for Vortex's own small jobs, such as a commit message.
    fn ask_once(&self, ask: OneShot) -> Pending<String> {
        drop(ask);
        Box::pin(async { anyhow::bail!("this agent cannot be asked for a single answer") })
    }

    /// The plan usage of the account `env` signs in with, from a process
    /// started for it alone rather than from a session.
    fn ask_usage(&self, env: Vec<(String, String)>) -> Pending<Usage> {
        drop(env);
        Box::pin(async { anyhow::bail!("this agent cannot be asked for usage on its own") })
    }
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
    /// The agent's own process, when it runs as one.
    pub pid: Option<u32>,
    pub events: async_channel::Receiver<AgentEvent>,
    pub commands: async_channel::Sender<SessionCommand>,
    pub driver: Pin<Box<dyn Future<Output = ()> + Send>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionCommand {
    Send(Message),
    Permission {
        request_id: String,
        decision: Decision,
    },
    Interrupt,
    /// Stops the subagent that tool call started, and those below it.
    StopSubagent(String),
    /// Asks for an [`AgentEvent::Recap`], as no turn of the conversation.
    /// None comes when the user turned recaps off in the agent.
    Recap,
    /// Switches the model for the following turns.
    SetModel(String),
    SetMode(PermissionMode),
    /// Sets the effort for the following turns; an
    /// [`AgentEvent::EffortChanged`] or [`AgentEvent::CommandFailed`] follows.
    SetEffort(Effort),
    /// Asks for a fresh [`AgentEvent::EffortChanged`].
    RefreshEffort,
    /// Asks for a fresh [`AgentEvent::ContextUsage`].
    RefreshContext,
    /// Asks for a fresh [`AgentEvent::Usage`].
    RefreshUsage,
    /// Asks for a fresh [`AgentEvent::ModelsChanged`].
    RefreshModels,
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
    /// The efforts the model takes, lowest first; empty when it takes none.
    pub efforts: Vec<Effort>,
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
        /// The version of the CLI the process runs, when it says.
        cli_version: Option<String>,
    },
    TextDelta {
        block: BlockKey,
        text: String,
    },
    ThinkingDelta {
        block: BlockKey,
        text: String,
    },
    /// About how many more tokens the model thought, for the thinking under
    /// way. The words of a thought are a summary; its tokens are not.
    ThinkingTokens(u64),
    /// How many tokens the thought in the message `message_id` took, as the
    /// agent counted once it was done; for an agent that gives no estimate
    /// as it thinks.
    ThoughtTokens {
        message_id: String,
        tokens: u64,
    },
    /// How long the model thought in the message `message_id`.
    ThinkingTime {
        message_id: String,
        duration: Duration,
    },
    ToolCall(ToolCall),
    /// A subagent's own tool calls and results come as `ToolCall` and
    /// `ToolResult`, with the subagent's call as their parent.
    Subagent(SubagentEvent),
    ToolResult {
        id: String,
        output: String,
        /// Pictures the tool returned, such as an image file it read.
        pictures: Vec<Picture>,
        is_error: bool,
        /// How a file tool changed the file, hunk by hunk; empty for other
        /// tools and for a new file.
        patch: Vec<Hunk>,
    },
    PermissionRequest(PermissionRequest),
    Ready(SessionInfo),
    /// The model in use changed; carries the concrete model id.
    ModelChanged(String),
    /// The models the user can pick from, as the agent now lists them.
    ModelsChanged(Vec<ModelOption>),
    ModeChanged(PermissionMode),
    /// The effort the next request runs with; `None` when the model takes
    /// none.
    EffortChanged(Option<Effort>),
    /// A model request of the conversation started; how its prompt was
    /// read. Subagents' requests are not reported.
    PromptTokens(PromptTokens),
    /// Tokens the model wrote in the request under way, thinking included.
    OutputTokens(u64),
    /// Everything the conversation spent so far, subagents included; comes
    /// as a turn ends.
    SpentTokens(SpentTokens),
    /// The last request of the conversation a session resumes, as the agent
    /// stored it, so the prompt cache shows before anything is sent.
    PastPromptTokens {
        tokens: PromptTokens,
        at: SystemTime,
    },
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
    /// How long the turn now ending took; not sent for a slash command the
    /// agent ran itself. `ended_at` is set for a stored turn.
    TurnTook {
        duration: Duration,
        ended_at: Option<SystemTime>,
    },
    /// Where the conversation stands, for a user coming back to it.
    Recap(String),
    /// How many tasks the agent runs in the background now, such as shell
    /// commands it left running; they end with its process.
    BackgroundTasks(usize),
    /// The message the user is likely to send next, predicted after a turn
    /// ends. None comes when the next step is not obvious.
    PromptSuggestion(String),
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

/// One hunk of a file change, as `git diff` shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hunk {
    /// The first line's number in the file before and after the change.
    pub old_start: usize,
    pub new_start: usize,
    /// Each line led by ` ` (unchanged), `-` (removed) or `+` (added).
    pub lines: Vec<String>,
}
