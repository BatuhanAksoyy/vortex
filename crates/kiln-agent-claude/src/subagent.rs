//! A subagent's life as the CLI reports it: the `task_*` lines of a task
//! the Agent tool started, and the messages the subagent writes, which
//! `--forward-subagent-text` adds to the stream.
//!
//! A subagent sent a message with `SendMessage` runs again as the same task,
//! but its start, progress and report name the `SendMessage` call; they are
//! reported under the call that first started it, which its messages name.

use std::{collections::HashMap, time::Duration};

use kiln_agent::{AgentEvent, SubagentChange, SubagentEnd, SubagentEvent, SubagentUsage};

use crate::{stream, wire};

/// The task type of a subagent; other tasks, such as a backgrounded shell
/// command, are not subagents.
const AGENT_TASK: &str = "local_agent";

/// The call that first started each task, by task id.
#[derive(Default)]
pub struct Tasks(HashMap<String, String>);

/// A task's start as the CLI reports it.
pub struct Start {
    pub task: Option<String>,
    pub call: Option<String>,
    pub description: String,
    pub kind: Option<String>,
    pub task_type: Option<String>,
    pub prompt: Option<String>,
}

/// A task's report as the CLI sends it.
pub struct Report<'a> {
    pub task: Option<&'a str>,
    pub call: Option<String>,
    pub status: &'a str,
    pub summary: String,
    pub usage: Option<&'a wire::TaskUsage>,
}

impl Tasks {
    pub fn started(&mut self, start: Start) -> Option<AgentEvent> {
        if start
            .task_type
            .is_some_and(|task_type| task_type != AGENT_TASK)
        {
            return None;
        }
        let call = start.call?;
        if let Some(first) = start.task.as_ref().and_then(|task| self.0.get(task))
            && *first != call
        {
            let message = start.prompt.unwrap_or_default();
            return Some(event(first.clone(), SubagentChange::Resumed { message }));
        }
        if let Some(task) = start.task {
            self.0.insert(task, call.clone());
        }
        let change = SubagentChange::Started {
            description: start.description,
            kind: start.kind,
        };
        Some(event(call, change))
    }

    pub fn progress(
        &self,
        task: Option<&str>,
        call: Option<String>,
        activity: String,
        usage: &wire::TaskUsage,
    ) -> Option<AgentEvent> {
        let usage = subagent_usage(usage);
        let change = SubagentChange::Progress { activity, usage };
        Some(event(self.first(task, call)?, change))
    }

    pub fn finished(&self, report: Report<'_>) -> Option<AgentEvent> {
        let change = SubagentChange::Finished {
            end: end(report.status),
            summary: report.summary,
            usage: report.usage.map(subagent_usage),
        };
        Some(event(self.first(report.task, report.call)?, change))
    }

    /// The call that first started `task`.
    pub fn call(&self, task: &str) -> Option<&str> {
        self.0.get(task).map(String::as_str)
    }

    /// The task `call` started.
    pub fn task(&self, call: &str) -> Option<&str> {
        let started = |(_, first): &(&String, &String)| *first == call;
        self.0.iter().find(started).map(|(task, _)| task.as_str())
    }

    /// The call that first started `task`, else the call the line names.
    fn first(&self, task: Option<&str>, call: Option<String>) -> Option<String> {
        task.and_then(|task| self.0.get(task)).cloned().or(call)
    }
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

/// How a task ended, by the status the CLI reports.
pub fn end(status: &str) -> SubagentEnd {
    match status {
        "completed" => SubagentEnd::Completed,
        "failed" => SubagentEnd::Failed,
        _ => SubagentEnd::Stopped,
    }
}

pub fn event(call: String, change: SubagentChange) -> AgentEvent {
    AgentEvent::Subagent(SubagentEvent { call, change })
}

fn subagent_usage(usage: &wire::TaskUsage) -> SubagentUsage {
    SubagentUsage {
        tokens: usage.total_tokens,
        tool_uses: usage.tool_uses,
        duration: Duration::from_millis(usage.duration_ms),
    }
}
