//! Tool calls and their results, as the CLI's messages carry them.

use vortex_agent::{AgentEvent, ToolCall};

use crate::wire;

/// The server-side tool through which the model consults a stronger one.
const ADVISOR: &str = "advisor";

/// The tool calls among `content`, made under `parent` if a subagent made
/// them. The advisor runs within the message, so its answer comes in it
/// too.
pub(crate) fn calls(content: Vec<wire::AssistantBlock>, parent: Option<&str>) -> Vec<AgentEvent> {
    content
        .into_iter()
        .filter_map(|block| match block {
            wire::AssistantBlock::ToolUse { id, name, input } => {
                Some(AgentEvent::ToolCall(ToolCall {
                    id,
                    parent: parent.map(str::to_owned),
                    name,
                    input,
                }))
            }
            // Other server tools answer in blocks Vortex does not read, and
            // would show as running for ever.
            wire::AssistantBlock::ServerToolUse { id, name, input } if name == ADVISOR => {
                Some(AgentEvent::ToolCall(ToolCall {
                    id,
                    parent: parent.map(str::to_owned),
                    name,
                    input,
                }))
            }
            wire::AssistantBlock::AdvisorToolResult {
                tool_use_id,
                content,
            } => Some(advice(tool_use_id, content)),
            wire::AssistantBlock::Text { .. }
            | wire::AssistantBlock::ServerToolUse { .. }
            | wire::AssistantBlock::Other => None,
        })
        .collect()
}

/// The advisor's answer as its call's result; empty when only the model
/// may read it.
fn advice(id: String, content: wire::AdvisorContent) -> AgentEvent {
    let (output, is_error) = match content {
        wire::AdvisorContent::AdvisorResult { text } => (text, false),
        wire::AdvisorContent::AdvisorToolResultError { error_code } => (error_code, true),
        wire::AdvisorContent::Other => (String::new(), false),
    };
    AgentEvent::ToolResult {
        id,
        output,
        pictures: Vec::new(),
        is_error,
        patch: Vec::new(),
    }
}

pub(crate) fn tool_results(message: wire::User) -> Vec<AgentEvent> {
    let wire::UserContent::Blocks(blocks) = message.message.content else {
        return Vec::new();
    };
    let mut patch = wire::ToolUseResult::patch(message.tool_use_result);
    blocks
        .into_iter()
        .filter_map(|block| match block {
            wire::UserBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => {
                let (output, pictures) = content.into_parts();
                Some(AgentEvent::ToolResult {
                    id: tool_use_id,
                    output,
                    pictures,
                    is_error,
                    // The CLI sends one result per message, so the patch is
                    // its only result's.
                    patch: std::mem::take(&mut patch),
                })
            }
            wire::UserBlock::Other => None,
        })
        .collect()
}
