//! A message the model wrote, or the output of a command the CLI ran.

use serde::Deserialize;
use serde_json::Value;

use super::PromptUsage;

#[derive(Debug, Deserialize)]
pub struct Assistant {
    pub message: AssistantMessage,
    pub parent_tool_use_id: Option<String>,
    /// Set on the output of a slash command the CLI ran itself; that output
    /// is never streamed.
    #[serde(default)]
    pub local_command_run: Option<CommandRun>,
    /// On the message that holds a thinking block: how long the model
    /// thought.
    #[serde(default)]
    pub thinking_duration_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct CommandRun {
    #[serde(default)]
    pub command: String,
}

#[derive(Debug, Deserialize)]
pub struct AssistantMessage {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub model: String,
    pub content: Vec<AssistantBlock>,
    #[serde(default)]
    pub usage: Option<PromptUsage>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AssistantBlock {
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    Text {
        text: String,
    },
    #[serde(other)]
    Other,
}
