//! Agent-neutral session model. Each backend (`kiln-agent-claude`, later
//! `kiln-agent-codex`) translates its own protocol into these types, so the UI
//! never sees protocol details.

use std::{future::Future, path::PathBuf, pin::Pin, time::SystemTime};

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
}

pub trait AgentBackend {
    fn spawn(&self, spec: SessionSpec) -> anyhow::Result<Session>;
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
}

/// How much the agent may do without asking.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    /// The short window (five hours for Claude).
    pub session: Option<UsageWindow>,
    pub weekly: Option<UsageWindow>,
    /// Set while a limit is reached and messages would be refused.
    pub limited: Option<Limited>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsageWindow {
    /// Whole percent used, 0–100.
    pub percent: u8,
    pub resets_at: Option<SystemTime>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limited {
    /// When sending works again, if the agent says.
    pub until: Option<SystemTime>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContextUsage {
    pub used_tokens: u64,
    pub max_tokens: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    Allow,
    /// Allow, and stop asking for calls like this one (see [`Remember`]).
    AllowAlways,
    /// Deny; `feedback` tells the agent what to do instead.
    Deny {
        feedback: Option<String>,
    },
}

/// What [`Decision::AllowAlways`] would remember, when the agent offers it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Remember {
    /// A permission rule for this kind of call, saved in project settings.
    Rule,
    /// Accept file edits without asking for the rest of the session.
    AcceptEdits,
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

#[derive(Clone, Debug, PartialEq)]
pub struct PermissionRequest {
    pub id: String,
    pub tool: String,
    pub input: Value,
    pub description: Option<String>,
    pub remember: Option<Remember>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TurnOutcome {
    Completed,
    Interrupted,
    Failed(String),
}
