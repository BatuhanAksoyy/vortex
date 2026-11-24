//! A stored conversation's lines as the messages a reopened chat shows.
//!
//! The lines form a tree: each names its parent, and editing an earlier
//! message starts a new branch in the same file. The conversation is the
//! branch that ends at the last message, walked back to its start.

use std::collections::{HashMap, HashSet};

use kiln_agent::{AgentEvent, BlockKey, Message, PastMessage, Picture, ToolCall, TurnOutcome};
use serde::Deserialize;
use serde_json::Value;

use crate::{
    picture,
    wire::{ImageSource, ToolOutput},
};

/// What the CLI writes as the user's message when a turn is interrupted.
const INTERRUPTED: &str = "[Request interrupted by user";

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Stored {
    #[serde(rename_all = "camelCase")]
    User {
        #[serde(default)]
        is_meta: bool,
        #[serde(default)]
        is_compact_summary: bool,
        message: UserMessage,
    },
    Assistant {
        uuid: String,
        message: AssistantMessage,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct UserMessage {
    content: UserContent,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum UserContent {
    Text(String),
    Blocks(Vec<UserBlock>),
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum UserBlock {
    Text {
        text: String,
    },
    Image {
        source: ImageSource,
    },
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

#[derive(Deserialize)]
struct AssistantMessage {
    content: Vec<AssistantBlock>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AssistantBlock {
    Text {
        text: String,
    },
    Thinking {
        thinking: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    #[serde(other)]
    Other,
}

pub fn messages(text: &str) -> Vec<PastMessage> {
    let lines: Vec<Value> = text
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    conversation(&lines)
        .into_iter()
        .filter_map(|line| Stored::deserialize(line).ok())
        .flat_map(shown)
        .collect()
}

/// The text a user line shows in a list: `None` for a line the user did
/// not type (tool results, the CLI's own notes, a compaction summary).
pub fn user_prompt(line: &Value) -> Option<String> {
    let Ok(Stored::User {
        is_meta: false,
        is_compact_summary: false,
        message,
    }) = Stored::deserialize(line)
    else {
        return None;
    };
    let text = match message.content {
        UserContent::Text(text) => text,
        UserContent::Blocks(blocks) => blocks.into_iter().find_map(|block| match block {
            UserBlock::Text { text } => Some(text),
            _ => None,
        })?,
    };
    match typed(&text)? {
        PastMessage::User(message) => Some(message.text),
        PastMessage::Agent(_) => None,
    }
}

/// The branch ending at the last message, oldest first. A compaction starts
/// a new chain whose first line still names the one before it.
fn conversation(lines: &[Value]) -> Vec<&Value> {
    let by_id: HashMap<&str, &Value> = lines
        .iter()
        .filter_map(|line| Some((field(line, "uuid")?, line)))
        .collect();
    let mut next = lines.iter().rev().find(|line| is_main_message(line));
    let mut seen = HashSet::new();
    let mut chain = Vec::new();
    while let Some(line) = next.filter(|line| seen.insert(field(line, "uuid"))) {
        chain.push(line);
        let parent = field(line, "parentUuid").or_else(|| field(line, "logicalParentUuid"));
        next = parent.and_then(|parent| by_id.get(parent).copied());
    }
    chain.reverse();
    chain
}

fn is_main_message(line: &Value) -> bool {
    matches!(field(line, "type"), Some("user" | "assistant"))
        && line.get("isSidechain") != Some(&Value::Bool(true))
}

fn field<'a>(line: &'a Value, key: &str) -> Option<&'a str> {
    line.get(key)?.as_str()
}

fn shown(line: Stored) -> Vec<PastMessage> {
    match line {
        Stored::User {
            is_meta: false,
            is_compact_summary: false,
            message,
        } => user_messages(message.content),
        Stored::Assistant { uuid, message } => assistant_messages(&uuid, message),
        Stored::User { .. } | Stored::Other => Vec::new(),
    }
}

fn user_messages(content: UserContent) -> Vec<PastMessage> {
    let blocks = match content {
        UserContent::Text(text) => return typed(&text).into_iter().collect(),
        UserContent::Blocks(blocks) => blocks,
    };
    let mut pictures: Vec<Picture> = Vec::new();
    let mut messages: Vec<PastMessage> = Vec::new();
    for block in blocks {
        match block {
            UserBlock::Text { text } => messages.extend(typed(&text)),
            UserBlock::Image { source } => pictures.extend(picture::decode(&source)),
            UserBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => {
                let (output, pictures) = content.into_parts();
                messages.push(PastMessage::Agent(AgentEvent::ToolResult {
                    id: tool_use_id,
                    output,
                    pictures,
                    is_error,
                }));
            }
            UserBlock::Other => {}
        }
    }
    attach(&mut messages, pictures);
    messages
}

/// Pictures sent with a message belong to its text; sent alone, they are
/// a message of their own.
fn attach(messages: &mut Vec<PastMessage>, pictures: Vec<Picture>) {
    if pictures.is_empty() {
        return;
    }
    match messages.iter_mut().find_map(|message| match message {
        PastMessage::User(message) => Some(message),
        PastMessage::Agent(_) => None,
    }) {
        Some(message) => message.pictures = pictures,
        None => messages.push(PastMessage::User(Message {
            pictures,
            ..Message::default()
        })),
    }
}

/// What a line of the user's text stands for: a message, a slash command
/// as typed, an interruption, or nothing worth showing (the CLI's notes).
fn typed(text: &str) -> Option<PastMessage> {
    let text = unwrap_pastes(text);
    let text = text.trim();
    if text.starts_with(INTERRUPTED) {
        return Some(PastMessage::Agent(AgentEvent::TurnEnded(
            TurnOutcome::Interrupted,
        )));
    }
    if let Some(name) = tagged(text, "command-name") {
        let command = match tagged(text, "command-args").filter(|args| !args.is_empty()) {
            Some(args) => format!("{name} {args}"),
            None => name.to_owned(),
        };
        return Some(PastMessage::User(command.into()));
    }
    if text.is_empty() || text.starts_with("<local-command-") || text.starts_with("<system-") {
        return None;
    }
    Some(PastMessage::User(text.into()))
}

/// Text pasted into Claude Code is stored wrapped in
/// `<pasted_content id="…">`; the user saw only the text.
fn unwrap_pastes(text: &str) -> String {
    const OPEN: &str = "<pasted_content";
    let mut rest = text.replace("</pasted_content>", "");
    while let Some(start) = rest.find(OPEN) {
        let Some(end) = rest[start..].find('>') else {
            break;
        };
        rest.replace_range(start..=start + end, "");
    }
    rest
}

/// The trimmed text between `<tag>` and `</tag>`.
fn tagged<'a>(text: &'a str, tag: &str) -> Option<&'a str> {
    let start = text.find(&format!("<{tag}>"))? + tag.len() + 2;
    let end = start + text[start..].find(&format!("</{tag}>"))?;
    Some(text[start..end].trim())
}

/// Each block keyed by its line, so every block is a row of its own, as it
/// was when streamed.
fn assistant_messages(uuid: &str, message: AssistantMessage) -> Vec<PastMessage> {
    (0..)
        .zip(message.content)
        .filter_map(|(index, block)| {
            let block_key = || BlockKey {
                message_id: uuid.to_owned(),
                index,
            };
            let event = match block {
                AssistantBlock::Text { text } if !text.trim().is_empty() => AgentEvent::TextDelta {
                    block: block_key(),
                    text,
                },
                // The CLI stores no thinking text unless asked to show it.
                AssistantBlock::Thinking { thinking } if !thinking.is_empty() => {
                    AgentEvent::ThinkingDelta {
                        block: block_key(),
                        text: thinking,
                    }
                }
                AssistantBlock::ToolUse { id, name, input } => AgentEvent::ToolCall(ToolCall {
                    id,
                    parent: None,
                    name,
                    input,
                }),
                _ => return None,
            };
            Some(PastMessage::Agent(event))
        })
        .collect()
}
