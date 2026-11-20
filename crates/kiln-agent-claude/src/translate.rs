//! Turns decoded CLI lines into `AgentEvent`s and session commands into CLI
//! messages. Pure and synchronous, so every fixture can be replayed in tests.

use std::collections::HashMap;

use kiln_agent::{
    AgentEvent, BlockKey, PermissionRequest, Remember, SessionCommand, ToolCall, TurnOutcome,
};
use serde_json::Value;

use crate::{
    control::{self, Asked},
    outgoing, usage,
    wire::{self, Line},
};

#[derive(Default)]
pub struct Translator {
    pending_permissions: HashMap<String, Pending>,
    /// Requests Kiln sent and awaits a reply to, by request id.
    asked: HashMap<String, Asked>,
    next_request: u64,
}

/// An open permission request: the tool input, echoed back on allow, and the
/// permission updates that "allow always" sends.
struct Pending {
    input: Value,
    remember: Option<Value>,
}

impl Translator {
    pub fn decode(line: &str) -> serde_json::Result<Line> {
        serde_json::from_str(line)
    }

    pub fn on_line(&mut self, line: &str) -> Vec<AgentEvent> {
        match Self::decode(line) {
            Ok(line) => self.on_decoded(line),
            Err(error) => {
                log::warn!("unreadable claude output ({error}): {line}");
                Vec::new()
            }
        }
    }

    /// The message to write for `command`, or `None` when it no longer applies
    /// (e.g. answering a permission request twice).
    pub fn on_command(&mut self, command: SessionCommand) -> Option<Value> {
        match command {
            SessionCommand::Send(text) => Some(outgoing::user_message(&text)),
            SessionCommand::Interrupt => Some(outgoing::interrupt(&self.request_id("interrupt"))),
            SessionCommand::SetModel(model) => {
                let id = self.ask("model", Asked::Model);
                Some(outgoing::set_model(&id, &model))
            }
            SessionCommand::SetMode(mode) => {
                let id = self.ask("mode", Asked::Mode);
                Some(outgoing::set_permission_mode(&id, control::mode_name(mode)))
            }
            SessionCommand::RefreshContext => {
                let id = self.ask("context", Asked::Context);
                Some(outgoing::get_context_usage(&id))
            }
            SessionCommand::RefreshUsage => {
                let id = self.ask("usage", Asked::Usage);
                Some(outgoing::get_usage(&id))
            }
            SessionCommand::Permission {
                request_id,
                decision,
            } => {
                let pending = self.pending_permissions.remove(&request_id)?;
                Some(outgoing::permission_response(
                    &request_id,
                    &decision,
                    &pending.input,
                    pending.remember.as_ref(),
                ))
            }
        }
    }

    fn request_id(&mut self, kind: &str) -> String {
        self.next_request += 1;
        format!("kiln-{kind}-{}", self.next_request)
    }

    fn ask(&mut self, kind: &str, asked: Asked) -> String {
        let id = self.request_id(kind);
        self.asked.insert(id.clone(), asked);
        id
    }

    fn on_decoded(&mut self, line: Line) -> Vec<AgentEvent> {
        match line {
            Line::System(wire::System::Init { session_id, model }) => {
                vec![AgentEvent::Started { session_id, model }]
            }
            Line::StreamEvent(event) => stream_event(event).into_iter().collect(),
            Line::Assistant(message) => tool_calls(message),
            Line::User(message) => tool_results(message),
            Line::ControlRequest(request) => self.control_request(request).into_iter().collect(),
            Line::RateLimitEvent { rate_limit_info } => usage::from_event(&rate_limit_info)
                .map(AgentEvent::Usage)
                .into_iter()
                .collect(),
            Line::ControlResponse(response) => self
                .control_response(response.response)
                .into_iter()
                .collect(),
            Line::System(wire::System::Status { permission_mode }) => permission_mode
                .as_deref()
                .and_then(control::parse_mode)
                .map(AgentEvent::ModeChanged)
                .into_iter()
                .collect(),
            Line::TurnResult(result) => vec![AgentEvent::TurnEnded(outcome(result))],
            Line::System(wire::System::Other) | Line::Other => Vec::new(),
        }
    }

    fn control_response(&mut self, reply: wire::Reply) -> Option<AgentEvent> {
        match reply {
            wire::Reply::Success {
                request_id,
                response,
            } => control::on_success(self.asked_by(&request_id)?, response),
            // Only changes the user asked for are worth showing as failed;
            // background refreshes run every turn and just keep the old
            // value.
            wire::Reply::Error { request_id, error } => match self.asked_by(&request_id)? {
                Asked::Model | Asked::Mode => Some(AgentEvent::CommandFailed(error)),
                asked => {
                    log::warn!("{asked:?} request failed: {error}");
                    None
                }
            },
            wire::Reply::Other => None,
        }
    }

    /// What the reply `request_id` answers; the initialize request is sent by
    /// the driver, not through a command, so it is known by its fixed id.
    fn asked_by(&mut self, request_id: &str) -> Option<Asked> {
        if request_id == outgoing::INITIALIZE_ID {
            return Some(Asked::Initialize);
        }
        self.asked.remove(request_id)
    }

    fn control_request(&mut self, request: wire::ControlRequest) -> Option<AgentEvent> {
        let wire::Request::CanUseTool {
            tool_name,
            input,
            description,
            permission_suggestions,
        } = request.request
        else {
            return None;
        };
        let (remember, updates) = remember(&permission_suggestions).unzip();
        let pending = Pending {
            input: input.clone(),
            remember: updates,
        };
        self.pending_permissions
            .insert(request.request_id.clone(), pending);
        Some(AgentEvent::PermissionRequest(PermissionRequest {
            id: request.request_id,
            tool: tool_name,
            input,
            description,
            remember,
        }))
    }
}

/// Subagent text is not forwarded by the CLI by default; deltas with a parent
/// are ignored until subagent transcripts are shown.
fn stream_event(event: wire::StreamEvent) -> Option<AgentEvent> {
    if event.parent_tool_use_id.is_some() {
        return None;
    }
    let wire::ApiEvent::ContentBlockDelta { index, delta } = event.event else {
        return None;
    };
    let block = BlockKey {
        message_id: event.api_message_id,
        index,
    };
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

fn tool_calls(message: wire::Assistant) -> Vec<AgentEvent> {
    let parent = message.parent_tool_use_id;
    message
        .message
        .content
        .into_iter()
        .filter_map(|block| match block {
            wire::AssistantBlock::ToolUse { id, name, input } => {
                Some(AgentEvent::ToolCall(ToolCall {
                    id,
                    parent: parent.clone(),
                    name,
                    input,
                }))
            }
            wire::AssistantBlock::Other => None,
        })
        .collect()
}

fn tool_results(message: wire::User) -> Vec<AgentEvent> {
    let wire::UserContent::Blocks(blocks) = message.message.content else {
        return Vec::new();
    };
    blocks
        .into_iter()
        .filter_map(|block| match block {
            wire::UserBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => Some(AgentEvent::ToolResult {
                id: tool_use_id,
                output: content.into_text(),
                is_error,
            }),
            wire::UserBlock::Other => None,
        })
        .collect()
}

fn outcome(result: wire::TurnResult) -> TurnOutcome {
    if result.subtype == "success" {
        return TurnOutcome::Completed;
    }
    if result
        .terminal_reason
        .as_deref()
        .is_some_and(|reason| reason.starts_with("aborted"))
    {
        return TurnOutcome::Interrupted;
    }
    TurnOutcome::Failed(result.result.unwrap_or(result.subtype))
}

/// The CLI suggests several updates per request. A saved rule is what "don't
/// ask again" means; when it offers none (file edits), switching the session
/// to accept edits is the equivalent.
fn remember(suggestions: &[Value]) -> Option<(Remember, Value)> {
    let of_type = |kind: &str| -> Vec<Value> {
        suggestions
            .iter()
            .filter(|suggestion| suggestion.get("type").and_then(Value::as_str) == Some(kind))
            .cloned()
            .collect()
    };
    let rules = of_type("addRules");
    if !rules.is_empty() {
        return Some((Remember::Rule, Value::Array(rules)));
    }
    let accept_edits: Vec<Value> = of_type("setMode")
        .into_iter()
        .filter(|suggestion| suggestion.get("mode").and_then(Value::as_str) == Some("acceptEdits"))
        .collect();
    (!accept_edits.is_empty()).then(|| (Remember::AcceptEdits, Value::Array(accept_edits)))
}
