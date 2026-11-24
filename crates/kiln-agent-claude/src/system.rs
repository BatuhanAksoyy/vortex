//! The CLI's `system` lines: the session starting, its state changing, the
//! commands on offer, and the subagents it runs.

use kiln_agent::AgentEvent;

use crate::{control, subagent, wire};

pub fn events(system: wire::System) -> Vec<AgentEvent> {
    match system {
        wire::System::Init { session_id, model } => {
            vec![AgentEvent::Started { session_id, model }]
        }
        wire::System::Status { permission_mode } => permission_mode
            .as_deref()
            .and_then(control::parse_mode)
            .map(AgentEvent::ModeChanged)
            .into_iter()
            .collect(),
        wire::System::CommandsChanged { commands } => {
            vec![AgentEvent::CommandsChanged(control::slash_commands(
                commands,
            ))]
        }
        wire::System::TaskStarted {
            tool_use_id,
            description,
            subagent_type,
            task_type,
        } => subagent::started(
            tool_use_id,
            description,
            subagent_type,
            task_type.as_deref(),
        )
        .into_iter()
        .collect(),
        wire::System::TaskProgress {
            tool_use_id,
            description,
            usage,
        } => subagent::progress(tool_use_id, description, &usage)
            .into_iter()
            .collect(),
        wire::System::TaskNotification {
            tool_use_id,
            status,
            summary,
            usage,
        } => subagent::finished(tool_use_id, &status, summary, usage.as_ref())
            .into_iter()
            .collect(),
        wire::System::Other => Vec::new(),
    }
}
