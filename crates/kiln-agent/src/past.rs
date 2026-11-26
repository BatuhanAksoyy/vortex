//! Conversations the agent stored, read back for the history list and for
//! reopening one.

use std::time::SystemTime;

use crate::{AgentEvent, Message};

/// A stored conversation, as the history list shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PastSession {
    /// What `SessionSpec::resume` takes.
    pub id: String,
    pub title: String,
    pub updated_at: SystemTime,
    /// Open in another client right now; sending to it from here as well
    /// would interleave both conversations.
    pub open_elsewhere: bool,
    /// The harness that stored it, by `AgentBackend::name`; only it can
    /// continue the conversation.
    pub agent: &'static str,
}

/// One message of a stored conversation. The agent's side is told as the
/// events a live turn would have sent.
#[derive(Clone, Debug, PartialEq)]
pub enum PastMessage {
    User(Message),
    Agent(AgentEvent),
}
