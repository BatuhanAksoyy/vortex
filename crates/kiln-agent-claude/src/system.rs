//! The CLI's `system` lines: the session starting, its state changing, the
//! commands on offer, and the subagents it runs.

use kiln_agent::AgentEvent;

use crate::{
    control,
    subagent::{Report, Start, Tasks},
    wire,
};

pub fn events(system: wire::System, tasks: &mut Tasks) -> Vec<AgentEvent> {
    let event = match system {
        wire::System::Init { session_id, model } => Some(AgentEvent::Started { session_id, model }),
        wire::System::Status { permission_mode } => permission_mode
            .as_deref()
            .and_then(control::parse_mode)
            .map(AgentEvent::ModeChanged),
        wire::System::CommandsChanged { commands } => Some(AgentEvent::CommandsChanged(
            control::slash_commands(commands),
        )),
        wire::System::TaskStarted {
            task_id,
            tool_use_id,
            description,
            prompt,
            subagent_type,
            task_type,
        } => tasks.started(Start {
            task: task_id,
            call: tool_use_id,
            description,
            kind: subagent_type,
            task_type,
            prompt,
        }),
        wire::System::TaskProgress {
            task_id,
            tool_use_id,
            description,
            usage,
        } => tasks.progress(task_id.as_deref(), tool_use_id, description, &usage),
        wire::System::TaskNotification {
            task_id,
            tool_use_id,
            status,
            summary,
            usage,
        } => tasks.finished(Report {
            task: task_id.as_deref(),
            call: tool_use_id,
            status: &status,
            summary,
            usage: usage.as_ref(),
        }),
        wire::System::Other => None,
    };
    event.into_iter().collect()
}
