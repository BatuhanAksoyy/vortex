//! Stored threads, read through the app-server, which owns their files and
//! its index of them: the history list and a reopened chat's messages.

use std::{path::Path, time::Duration};

use serde_json::json;
use vortex_agent::{AgentEvent, BlockKey, Message, PastMessage, PastSession};

use crate::{
    client::Client,
    items,
    protocol::{Thread, ThreadItem, ThreadListResponse, ThreadReadResponse, Turn, UserInput},
    report,
};

/// Threads listed at most; the history shows the recent ones.
const LISTED: u64 = 100;

pub async fn list(program: &Path, cwd: &Path) -> anyhow::Result<Vec<PastSession>> {
    let mut client = Client::start(program, Vec::new(), None).await?;
    let params = json!({ "cwd": cwd.to_string_lossy(), "limit": LISTED });
    let listed: ThreadListResponse =
        serde_json::from_value(client.request("thread/list", params).await?)?;
    Ok(listed.data.iter().map(session).collect())
}

fn session(thread: &Thread) -> PastSession {
    let title = thread.name.clone().filter(|name| !name.is_empty());
    PastSession {
        id: thread.id.clone(),
        title: title.unwrap_or_else(|| thread.preview.clone()),
        updated_at: report::at(thread.updated_at),
        open_elsewhere: false,
        agent: "codex",
    }
}

pub async fn read(program: &Path, thread_id: &str) -> anyhow::Result<Vec<PastMessage>> {
    let mut client = Client::start(program, Vec::new(), None).await?;
    let params = json!({ "threadId": thread_id, "includeTurns": true });
    let read: ThreadReadResponse =
        serde_json::from_value(client.request("thread/read", params).await?)?;
    Ok(read.thread.turns.iter().flat_map(turn_messages).collect())
}

/// A stored turn told as a live one would have shown it.
pub fn turn_messages(turn: &Turn) -> Vec<PastMessage> {
    let mut messages: Vec<PastMessage> = turn.items.iter().flat_map(item_messages).collect();
    if let Some(took) = turn.duration_ms.and_then(|ms| u64::try_from(ms).ok()) {
        messages.push(PastMessage::Agent(AgentEvent::TurnTook {
            duration: Duration::from_millis(took),
            ended_at: turn.completed_at.map(report::at),
        }));
    }
    messages
}

fn item_messages(item: &ThreadItem) -> Vec<PastMessage> {
    let block = |id: &str| BlockKey {
        message_id: id.to_owned(),
        index: 0,
    };
    let agent = |event| vec![PastMessage::Agent(event)];
    match item {
        ThreadItem::UserMessage(message) => vec![PastMessage::User(user_message(&message.content))],
        ThreadItem::AgentMessage(message) => agent(AgentEvent::TextDelta {
            block: block(&message.id),
            text: message.text.clone(),
        }),
        ThreadItem::Reasoning(thought) => {
            let summary = thought.summary.as_deref().unwrap_or_default().join("\n\n");
            if summary.is_empty() {
                return Vec::new();
            }
            agent(AgentEvent::ThinkingDelta {
                block: block(&thought.id),
                text: summary,
            })
        }
        _ => (items::started(item).into_iter())
            .chain(items::completed(item))
            .map(PastMessage::Agent)
            .collect(),
    }
}

fn user_message(content: &[UserInput]) -> Message {
    let mut message = Message::default();
    for input in content {
        match input {
            UserInput::Text(text) => message.text.push_str(&text.text),
            UserInput::LocalImage(image) => message.files.push(image.path.clone().into()),
            UserInput::Mention(mention) => message.files.push(mention.path.clone().into()),
            _ => {}
        }
    }
    message
}
