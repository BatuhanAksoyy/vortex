//! Kiln's own small questions to a model, outside any session.

use std::path::PathBuf;

/// A question to a model, answered once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OneShot {
    pub cwd: PathBuf,
    /// The account's environment, as a session of it gets.
    pub env: Vec<(String, String)>,
    pub model: String,
    /// What the model is, in place of the agent's own system prompt.
    pub system: String,
    pub prompt: String,
}
