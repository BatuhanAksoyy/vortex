use std::time::Duration;

use crate::PromptTokens;

/// Something that happened to the subagent the tool call `call` started.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubagentEvent {
    pub call: String,
    pub change: SubagentChange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SubagentChange {
    Started {
        description: String,
        /// The kind of agent, e.g. `general-purpose`, when the agent names it.
        kind: Option<String>,
    },
    /// It was sent `message` after it had ended or while it ran, and runs
    /// again to answer it.
    Resumed { message: String },
    /// The model a message of the subagent was written with.
    Model(String),
    /// Text the subagent wrote, whole.
    Text(String),
    /// The prompt of a request the subagent sent; its size is how much of
    /// the context window the subagent fills.
    Request(PromptTokens),
    /// What it is doing now, and what it has used so far.
    Progress {
        activity: String,
        usage: SubagentUsage,
    },
    Finished {
        end: SubagentEnd,
        /// Its report, or why it ended.
        summary: String,
        usage: Option<SubagentUsage>,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SubagentUsage {
    pub tokens: u64,
    pub tool_uses: u64,
    pub duration: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubagentEnd {
    Completed,
    Failed,
    Stopped,
}
