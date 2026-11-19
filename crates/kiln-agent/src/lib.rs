//! Agent-neutral session model. Each backend (`kiln-agent-claude`, later
//! `kiln-agent-codex`) translates its own protocol into these types, so the UI
//! never sees protocol details.

use std::{future::Future, path::PathBuf, pin::Pin};

use serde_json::Value;

/// Where and how to start a session.
#[derive(Clone, Debug)]
pub struct SessionSpec {
    pub cwd: PathBuf,
    /// Extra environment, e.g. the account's config directory.
    pub env: Vec<(String, String)>,
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Deny,
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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TurnOutcome {
    Completed,
    Interrupted,
    Failed(String),
}
