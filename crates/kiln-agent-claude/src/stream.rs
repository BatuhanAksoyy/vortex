//! The API events the CLI forwards as a request streams in: how its prompt
//! was read, and text and thinking as they arrive.

use std::time::Duration;

use kiln_agent::{AgentEvent, BlockKey, PromptTokens};

use crate::wire;

/// Subagent requests are not reported: their text is not forwarded by the
/// CLI by default, and their prompts are cached apart from the
/// conversation's.
pub fn event(event: wire::StreamEvent) -> Option<AgentEvent> {
    if event.parent_tool_use_id.is_some() {
        return None;
    }
    match event.event {
        wire::ApiEvent::MessageStart { message } => {
            Some(AgentEvent::PromptTokens(prompt_tokens(&message.usage)))
        }
        wire::ApiEvent::ContentBlockDelta { index, delta } => {
            let block = BlockKey {
                message_id: event.api_message_id,
                index,
            };
            delta_event(block, delta)
        }
        wire::ApiEvent::Other => None,
    }
}

fn delta_event(block: BlockKey, delta: wire::Delta) -> Option<AgentEvent> {
    match delta {
        wire::Delta::Text { text } => Some(AgentEvent::TextDelta { block, text }),
        wire::Delta::Thinking { thinking } if !thinking.is_empty() => {
            Some(AgentEvent::ThinkingDelta {
                block,
                text: thinking,
            })
        }
        wire::Delta::Thinking { .. } | wire::Delta::Other => None,
    }
}

pub fn prompt_tokens(usage: &wire::PromptUsage) -> PromptTokens {
    let written = &usage.cache_creation;
    let kept_for = if written.ephemeral_1h_input_tokens > 0 {
        Some(Duration::from_hours(1))
    } else if written.ephemeral_5m_input_tokens > 0 {
        Some(Duration::from_mins(5))
    } else {
        None
    };
    PromptTokens {
        cached: usage.cache_read_input_tokens,
        written: usage.cache_creation_input_tokens,
        uncached: usage.input_tokens,
        kept_for,
    }
}
