//! A tool call as the agent reports it, and how a file tool changed a file.

use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct ToolCall {
    pub id: String,
    /// The tool call of the subagent that made this call, if any.
    pub parent: Option<String>,
    pub name: String,
    pub input: Value,
}

/// One hunk of a file change, as `git diff` shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hunk {
    /// The first line's number in the file before and after the change.
    pub old_start: usize,
    pub new_start: usize,
    /// Each line led by ` ` (unchanged), `-` (removed) or `+` (added).
    pub lines: Vec<String>,
}
