//! How much of the plan's limits and of the context window is spent.

use std::time::SystemTime;

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
