//! A user line: the tool results the CLI sends back, and what a tool
//! reports beside its text in `tool_use_result` (stored as `toolUseResult`),
//! for a file tool the patch the CLI shows.

use serde::Deserialize;

use super::ToolOutput;

#[derive(Debug, Deserialize)]
pub struct User {
    pub message: UserMessage,
    /// What the tool reports beside its text, such as a file tool's patch.
    #[serde(default)]
    pub tool_use_result: Option<ToolUseResult>,
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

/// Only an object carries a patch; some tools report a bare string.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum ToolUseResult {
    Fields(ToolUseFields),
    Other(serde::de::IgnoredAny),
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolUseFields {
    #[serde(default)]
    structured_patch: Vec<PatchHunk>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PatchHunk {
    old_start: usize,
    new_start: usize,
    lines: Vec<String>,
}

impl ToolUseResult {
    /// The patch of a file tool's result; empty for any other.
    pub fn patch(result: Option<Self>) -> Vec<kiln_agent::Hunk> {
        let Some(Self::Fields(fields)) = result else {
            return Vec::new();
        };
        let hunk = |hunk: PatchHunk| kiln_agent::Hunk {
            old_start: hunk.old_start,
            new_start: hunk.new_start,
            lines: hunk.lines,
        };
        fields.structured_patch.into_iter().map(hunk).collect()
    }
}
