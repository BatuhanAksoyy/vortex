//! A subagent's life as the CLI reports it: the `task_*` lines of a task
//! the Agent tool started, and the messages the subagent writes, which
//! `--forward-subagent-text` adds to the stream.

use std::time::Duration;

use kiln_agent::{AgentEvent, SubagentChange, SubagentEnd, SubagentEvent, SubagentUsage};

use crate::{stream, wire};

/// The task type of a subagent; other tasks, such as a backgrounded shell
/// command, are not subagents.
const AGENT_TASK: &str = "local_agent";

pub fn started(
    call: Option<String>,
    description: String,
    kind: Option<String>,
    task_type: Option<&str>,
) -> Option<AgentEvent> {
    if task_type.is_some_and(|task_type| task_type != AGENT_TASK) {
        return None;
    }
    Some(event(call?, SubagentChange::Started { description, kind }))
}

pub fn progress(
    call: Option<String>,
    activity: String,
    usage: &wire::TaskUsage,
) -> Option<AgentEvent> {
    let usage = subagent_usage(usage);
    Some(event(call?, SubagentChange::Progress { activity, usage }))
}

pub fn finished(
    call: Option<String>,
    status: &str,
    summary: String,
    usage: Option<&wire::TaskUsage>,
) -> Option<AgentEvent> {
    let end = match status {
        "completed" => SubagentEnd::Completed,
        "failed" => SubagentEnd::Failed,
        _ => SubagentEnd::Stopped,
    };
    let usage = usage.map(subagent_usage);
    let change = SubagentChange::Finished {
        end,
        summary,
        usage,
    };
    Some(event(call?, change))
}

/// The model, request and text of a message the subagent under `call`
/// wrote; its tool calls are read like the main agent's.
pub fn message(call: &str, message: &wire::AssistantMessage) -> Vec<AgentEvent> {
    let model = (!message.model.is_empty()).then(|| SubagentChange::Model(message.model.clone()));
    let request = message
        .usage
        .as_ref()
        .map(|usage| SubagentChange::Request(stream::prompt_tokens(usage)));
    let texts = message.content.iter().filter_map(|block| match block {
        wire::AssistantBlock::Text { text } if !text.trim().is_empty() => {
            Some(SubagentChange::Text(text.clone()))
        }
        _ => None,
    });
    model
        .into_iter()
        .chain(request)
        .chain(texts)
        .map(|change| event(call.to_owned(), change))
        .collect()
}

fn event(call: String, change: SubagentChange) -> AgentEvent {
    AgentEvent::Subagent(SubagentEvent { call, change })
}

fn subagent_usage(usage: &wire::TaskUsage) -> SubagentUsage {
    SubagentUsage {
        tokens: usage.total_tokens,
        tool_uses: usage.tool_uses,
        duration: Duration::from_millis(usage.duration_ms),
    }
}
