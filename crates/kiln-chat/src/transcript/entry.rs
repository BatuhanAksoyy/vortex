//! What the transcript lists: one entry per message, thought, tool call or
//! notice.

use std::time::Duration;

use kiln_agent::{BlockKey, Hunk, Message, Picture, ToolCall};

#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    User(Message),
    Text {
        block: BlockKey,
        text: String,
    },
    Thinking {
        block: BlockKey,
        /// A summary of the thought, as the CLI sends it.
        text: String,
        /// About how many tokens the model thought.
        tokens: u64,
        /// How long it thought, once it is done.
        took: Option<Duration>,
    },
    Tool {
        call: ToolCall,
        result: Option<ToolResult>,
    },
    /// The call that started a subagent, once the agent reports it running;
    /// the subagent has the entry's id.
    Subagent,
    Notice {
        text: String,
        is_error: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolResult {
    pub output: String,
    pub pictures: Vec<Picture>,
    pub is_error: bool,
    /// How a file tool changed the file, as `git diff` shows it.
    pub patch: Vec<Hunk>,
}
