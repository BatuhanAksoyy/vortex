//! The subagents a stored conversation ran. The CLI keeps each one's own
//! conversation in `<session id>/subagents/agent-<agent id>.jsonl`, beside a
//! `.meta.json` naming the call that started it, and writes each report into
//! the main conversation as a `<task-notification>` message.
//!
//! A subagent sent a message with `SendMessage` goes on in the same file; a
//! `coordinator` line marks where the message reached it. Its steps up to
//! that line are woven in after the call that started it, and the rest after
//! the `SendMessage` call, so they read in the order they happened.

use std::{
    collections::{HashMap, VecDeque},
    path::Path,
    time::Duration,
};

use serde::Deserialize;
use vortex_agent::{
    AgentEvent, PastMessage, SubagentChange, SubagentEnd, SubagentEvent, SubagentUsage, ToolCall,
};

use super::messages::tagged;
use crate::{subagent, translate, wire};

pub struct StoredAgent {
    id: String,
    call: String,
    description: String,
    kind: Option<String>,
    /// Whether it ran to its end, as the CLI marks it.
    finished: bool,
    /// The last text it wrote, its report if none is stored.
    last_text: Option<String>,
    /// Its events up to each message it was sent, and after the last.
    segments: VecDeque<Vec<AgentEvent>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Meta {
    tool_use_id: String,
    #[serde(default)]
    description: String,
    agent_type: Option<String>,
    #[serde(default)]
    finished: bool,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Line {
    User {
        #[serde(default)]
        origin: Option<Origin>,
        message: wire::UserMessage,
        #[serde(default, rename = "toolUseResult")]
        tool_use_result: Option<wire::ToolUseResult>,
    },
    Assistant {
        message: wire::AssistantMessage,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct Origin {
    kind: String,
}

/// The subagents stored in `dir`; one whose files cannot be read is left
/// out, and its call shows as a plain tool row.
pub fn load(dir: &Path) -> Vec<StoredAgent> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            let id = name
                .to_str()?
                .strip_prefix("agent-")?
                .strip_suffix(".meta.json")?;
            let meta = std::fs::read_to_string(entry.path()).ok()?;
            let text = std::fs::read_to_string(dir.join(format!("agent-{id}.jsonl"))).ok()?;
            parse(id, &meta, &text)
        })
        .collect()
}

/// Subagent `id` from its `.meta.json` and conversation.
pub fn parse(id: &str, meta: &str, text: &str) -> Option<StoredAgent> {
    let meta: Meta = serde_json::from_str(meta).ok()?;
    let segments = segments(&meta.tool_use_id, text);
    let last_text = segments
        .iter()
        .flatten()
        .rev()
        .find_map(|event| match event {
            AgentEvent::Subagent(SubagentEvent {
                change: SubagentChange::Text(text),
                ..
            }) => Some(text.clone()),
            _ => None,
        });
    Some(StoredAgent {
        id: id.to_owned(),
        call: meta.tool_use_id,
        description: meta.description,
        kind: meta.agent_type,
        finished: meta.finished,
        last_text,
        segments,
    })
}

fn segments(call: &str, text: &str) -> VecDeque<Vec<AgentEvent>> {
    let mut segments = VecDeque::from([Vec::new()]);
    for line in text.lines() {
        let Ok(line) = serde_json::from_str::<Line>(line) else {
            continue;
        };
        let events = match line {
            Line::User {
                origin: Some(origin),
                ..
            } if origin.kind == "coordinator" => {
                segments.push_back(Vec::new());
                continue;
            }
            Line::User {
                message,
                tool_use_result,
                ..
            } => translate::tool_results(wire::User {
                message,
                tool_use_result,
            }),
            Line::Assistant { message } => {
                let mut events = subagent::message(call, &message);
                events.extend(translate::calls(message.content, Some(call)));
                events
            }
            Line::Other => continue,
        };
        if let Some(segment) = segments.back_mut() {
            segment.extend(events);
        }
    }
    segments
}

/// `messages` with each subagent's start and steps after the calls that
/// started it or sent it a message, and its reports on its card.
pub fn weave(messages: Vec<PastMessage>, agents: Vec<StoredAgent>) -> Vec<PastMessage> {
    let mut agents: HashMap<String, StoredAgent> = agents
        .into_iter()
        .map(|agent| (agent.id.clone(), agent))
        .collect();
    let by_call: HashMap<String, String> = agents
        .values()
        .map(|agent| (agent.call.clone(), agent.id.clone()))
        .collect();
    let mut woven = Vec::with_capacity(messages.len());
    for message in messages {
        let after = match &message {
            PastMessage::Agent(AgentEvent::ToolCall(call)) => {
                following(call, &by_call, &mut agents)
            }
            _ => Vec::new(),
        };
        woven.push(on_card(message, &agents));
        woven.extend(after.into_iter().map(PastMessage::Agent));
    }
    let unreported = unreported(&woven, agents.into_values());
    woven.extend(unreported);
    woven
}

/// A report for each subagent the conversation holds none for: done with
/// its last text if the CLI marked it finished, else stopped, since none
/// runs on once its conversation is reopened.
fn unreported(
    woven: &[PastMessage],
    agents: impl Iterator<Item = StoredAgent>,
) -> Vec<PastMessage> {
    let reported = |call: &str| {
        woven.iter().any(|message| {
            matches!(
                message,
                PastMessage::Agent(AgentEvent::Subagent(SubagentEvent {
                    call: of,
                    change: SubagentChange::Finished { .. },
                })) if of == call
            )
        })
    };
    agents
        .filter(|agent| !reported(&agent.call))
        .map(|agent| {
            let change = SubagentChange::Finished {
                end: if agent.finished {
                    SubagentEnd::Completed
                } else {
                    SubagentEnd::Stopped
                },
                summary: agent.last_text.unwrap_or_default(),
                usage: None,
            };
            PastMessage::Agent(subagent::event(agent.call, change))
        })
        .collect()
}

/// What follows `call`: the start of the subagent it started, or the
/// message it sent one, with the subagent's steps up to its next message.
fn following(
    call: &ToolCall,
    by_call: &HashMap<String, String>,
    agents: &mut HashMap<String, StoredAgent>,
) -> Vec<AgentEvent> {
    if let Some(agent) = by_call.get(&call.id).and_then(|id| agents.get_mut(id)) {
        let started = SubagentChange::Started {
            description: agent.description.clone(),
            kind: agent.kind.clone(),
        };
        let mut events = vec![subagent::event(agent.call.clone(), started)];
        let steps = agent.segments.pop_front().unwrap_or_default();
        events.extend(expand(steps, by_call, agents));
        return events;
    }
    let to = call.input.get("to").and_then(|to| to.as_str());
    let Some(agent) = to
        .filter(|_| call.name == "SendMessage")
        .and_then(|to| agents.get_mut(to))
    else {
        return Vec::new();
    };
    let message = call
        .input
        .get("message")
        .and_then(|message| message.as_str());
    let resumed = SubagentChange::Resumed {
        message: message.unwrap_or_default().to_owned(),
    };
    let mut events = vec![subagent::event(agent.call.clone(), resumed)];
    let steps = agent.segments.pop_front().unwrap_or_default();
    events.extend(expand(steps, by_call, agents));
    events
}

/// A subagent's steps, each followed by those of the subagent it started or
/// sent a message.
fn expand(
    steps: Vec<AgentEvent>,
    by_call: &HashMap<String, String>,
    agents: &mut HashMap<String, StoredAgent>,
) -> Vec<AgentEvent> {
    let mut events = Vec::with_capacity(steps.len());
    for step in steps {
        let after = match &step {
            AgentEvent::ToolCall(call) => following(call, by_call, agents),
            _ => Vec::new(),
        };
        events.push(step);
        events.extend(after);
    }
    events
}

/// A report names the subagent by its id; its card is under the call that
/// started it.
fn on_card(message: PastMessage, agents: &HashMap<String, StoredAgent>) -> PastMessage {
    match message {
        PastMessage::Agent(AgentEvent::Subagent(mut event)) => {
            if let Some(agent) = agents.get(&event.call) {
                event.call.clone_from(&agent.call);
            }
            PastMessage::Agent(AgentEvent::Subagent(event))
        }
        message => message,
    }
}

/// A `<task-notification>` the CLI wrote into the conversation: the report
/// of the subagent it names, by its id or else by the call it ran under.
pub fn report(text: &str) -> Option<PastMessage> {
    let call = tagged(text, "task-id").or_else(|| tagged(text, "tool-use-id"))?;
    let summary = tagged(text, "result")
        .or_else(|| tagged(text, "summary"))
        .unwrap_or_default();
    let number = |tag: &str| tagged(text, tag)?.parse::<u64>().ok();
    let usage = number("subagent_tokens").map(|tokens| SubagentUsage {
        tokens,
        tool_uses: number("tool_uses").unwrap_or_default(),
        duration: Duration::from_millis(number("duration_ms").unwrap_or_default()),
    });
    let change = SubagentChange::Finished {
        end: subagent::end(tagged(text, "status").unwrap_or_default()),
        summary: summary.to_owned(),
        usage,
    };
    Some(PastMessage::Agent(subagent::event(call.to_owned(), change)))
}
