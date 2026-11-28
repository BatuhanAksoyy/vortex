//! Tool calls and their results, as the CLI's messages carry them.

use vortex_agent::{AgentEvent, ToolCall};

use crate::wire;

/// The tool calls among `content`, made under `parent` if a subagent made
/// them.
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
            wire::AssistantBlock::Text { .. } | wire::AssistantBlock::Other => None,
        })
        .collect()
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
