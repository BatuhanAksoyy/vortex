//! The subset of Claude Code's `stream-json` output that Kiln reads. Unknown
//! line types and subtypes decode to `Other` so new CLI versions degrade
//! gracefully instead of failing.

use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Line {
    System(System),
    StreamEvent(StreamEvent),
    Assistant(Assistant),
    User(User),
    ControlRequest(ControlRequest),
    #[serde(rename = "result")]
    TurnResult(TurnResult),
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "subtype", rename_all = "snake_case")]
pub enum System {
    Init {
        session_id: String,
        model: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
pub struct StreamEvent {
    pub event: ApiEvent,
    pub parent_tool_use_id: Option<String>,
    #[serde(default)]
    pub api_message_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ApiEvent {
    ContentBlockDelta {
        index: u64,
        delta: Delta,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Delta {
    #[serde(rename = "text_delta")]
    Text { text: String },
    #[serde(rename = "thinking_delta")]
    Thinking { thinking: String },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
pub struct Assistant {
    pub message: AssistantMessage,
    pub parent_tool_use_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AssistantMessage {
    pub content: Vec<AssistantBlock>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AssistantBlock {
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
pub struct User {
    pub message: UserMessage,
}

#[derive(Debug, Deserialize)]
pub struct UserMessage {
    pub content: UserContent,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum UserContent {
    Text(String),
    Blocks(Vec<UserBlock>),
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UserBlock {
    ToolResult {
        tool_use_id: String,
        #[serde(default)]
        content: ToolOutput,
        #[serde(default)]
        is_error: bool,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum ToolOutput {
    Text(String),
    Parts(Vec<OutputPart>),
}

impl Default for ToolOutput {
    fn default() -> Self {
        Self::Text(String::new())
    }
}

impl ToolOutput {
    pub fn into_text(self) -> String {
        match self {
            Self::Text(text) => text,
            Self::Parts(parts) => parts
                .into_iter()
                .filter_map(|part| match part {
                    OutputPart::Text { text } => Some(text),
                    OutputPart::Other => None,
                })
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutputPart {
    Text {
        text: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
pub struct ControlRequest {
    pub request_id: String,
    pub request: Request,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "subtype", rename_all = "snake_case")]
pub enum Request {
    CanUseTool {
        tool_name: String,
        input: Value,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        permission_suggestions: Vec<Value>,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
pub struct TurnResult {
    pub subtype: String,
    #[serde(default)]
    pub result: Option<String>,
    #[serde(default)]
    pub terminal_reason: Option<String>,
}
