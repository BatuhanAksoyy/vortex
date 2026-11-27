//! The subset of Claude Code's `stream-json` output that Vortex reads. Unknown
//! line types and subtypes decode to `Other` so new CLI versions degrade
//! gracefully instead of failing.

mod user;

pub use user::{ToolUseResult, User, UserBlock, UserContent, UserMessage};

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::Value;
use vortex_agent::Picture;

use crate::picture;

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
    /// What the user may send next, predicted after a turn.
    PromptSuggestion {
        suggestion: String,
    },
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
        #[serde(default)]
        claude_code_version: Option<String>,
    },
    /// Sent when the session's state changes, e.g. its permission mode.
    Status {
        #[serde(default, rename = "permissionMode")]
        permission_mode: Option<String>,
    },
    CommandsChanged {
        commands: Vec<WireCommand>,
    },
    /// A background task started: a subagent, but also e.g. a backgrounded
    /// shell command.
    TaskStarted {
        task_id: Option<String>,
        tool_use_id: Option<String>,
        #[serde(default)]
        description: String,
        prompt: Option<String>,
        subagent_type: Option<String>,
        task_type: Option<String>,
    },
    TaskProgress {
        task_id: Option<String>,
        tool_use_id: Option<String>,
        #[serde(default)]
        description: String,
        usage: TaskUsage,
    },
    TaskNotification {
        task_id: Option<String>,
        tool_use_id: Option<String>,
        status: String,
        #[serde(default)]
        summary: String,
        usage: Option<TaskUsage>,
    },
    /// Every task running in the background, each time the list changes.
    BackgroundTasksChanged {
        #[serde(default)]
        tasks: Vec<Value>,
    },
    /// An estimate of the tokens thought since the last one, while the
    /// model thinks.
    ThinkingTokens {
        #[serde(default)]
        estimated_tokens_delta: u64,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
pub struct TaskUsage {
    #[serde(default)]
    pub total_tokens: u64,
    #[serde(default)]
    pub tool_uses: u64,
    #[serde(default)]
    pub duration_ms: u64,
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
    /// Ends a message, with how many tokens it took.
    MessageDelta {
        #[serde(default)]
        usage: WrittenUsage,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
pub struct StartedMessage {
    #[serde(default)]
    pub usage: PromptUsage,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct WrittenUsage {
    pub output_tokens: u64,
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
    /// Its text, and the pictures among its parts.
    pub fn into_parts(self) -> (String, Vec<Picture>) {
        let parts = match self {
            Self::Text(text) => return (text, Vec::new()),
            Self::Parts(parts) => parts,
        };
        let mut texts = Vec::new();
        let mut pictures = Vec::new();
        for part in parts {
            match part {
                OutputPart::Text { text } => texts.push(text),
                OutputPart::Image { source } => pictures.extend(picture::decode(&source)),
                OutputPart::Other => {}
            }
        }
        (texts.join("\n"), pictures)
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutputPart {
    Text {
        text: String,
    },
    Image {
        source: ImageSource,
    },
    #[serde(other)]
    Other,
}

/// A picture's bytes as the API carries them; only `base64` sources hold
/// them.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ImageSource {
    #[serde(rename = "type")]
    pub kind: String,
    pub media_type: String,
    pub data: String,
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
        agent_id: Option<String>,
    },
    #[serde(other)]
    Other,
}

/// The CLI's answer to a request Vortex sent.
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
    #[serde(default)]
    pub duration_ms: u64,
    /// Zero for a slash command the CLI ran itself.
    #[serde(default)]
    pub num_turns: u64,
    /// What the conversation spent so far, by model.
    #[serde(default, rename = "modelUsage")]
    pub model_usage: HashMap<String, ModelUsage>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ModelUsage {
    #[serde(rename = "inputTokens")]
    pub input: u64,
    #[serde(rename = "outputTokens")]
    pub output: u64,
    #[serde(rename = "cacheReadInputTokens")]
    pub cached: u64,
    #[serde(rename = "cacheCreationInputTokens")]
    pub written: u64,
}
