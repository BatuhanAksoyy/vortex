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
    ControlResponse(ControlResponse),
    RateLimitEvent {
        rate_limit_info: Value,
    },
    #[serde(rename = "result")]
    TurnResult(TurnResult),
    /// The conversation was emptied; a new `init` follows. `trigger` says
    /// why: `clear` for `/clear` and its aliases.
    ConversationReset {
        #[serde(default)]
        trigger: String,
    },
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
    /// Sent when the session's state changes, e.g. its permission mode.
    Status {
        #[serde(default, rename = "permissionMode")]
        permission_mode: Option<String>,
    },
    CommandsChanged {
        commands: Vec<WireCommand>,
    },
    #[serde(other)]
    Other,
}

/// A slash command as the CLI lists it, in the initialize reply and in
/// `commands_changed`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireCommand {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub argument_hint: String,
    #[serde(default)]
    pub aliases: Vec<String>,
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
    MessageStart {
        message: StartedMessage,
    },
    ContentBlockDelta {
        index: u64,
        delta: Delta,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
pub struct StartedMessage {
    #[serde(default)]
    pub usage: PromptUsage,
}

/// How the request's prompt was billed, as the API reports it.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct PromptUsage {
    pub input_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub cache_creation: CacheCreation,
}

/// What was written to the cache, by how long it is kept.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct CacheCreation {
    pub ephemeral_5m_input_tokens: u64,
    pub ephemeral_1h_input_tokens: u64,
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
    /// Set on the output of a slash command the CLI ran itself; that output
    /// is never streamed.
    #[serde(default)]
    pub local_command_run: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct AssistantMessage {
    #[serde(default)]
    pub id: String,
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
    Text {
        text: String,
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

/// The CLI's answer to a request Kiln sent.
#[derive(Debug, Deserialize)]
pub struct ControlResponse {
    pub response: Reply,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "subtype", rename_all = "snake_case")]
pub enum Reply {
    Success {
        request_id: String,
        #[serde(default)]
        response: Value,
    },
    Error {
        request_id: String,
        #[serde(default)]
        error: String,
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
    /// Set on turns the API refused, which still end with `success`.
    #[serde(default)]
    pub is_error: bool,
}
