//! Turns decoded CLI lines into `AgentEvent`s and session commands into CLI
//! messages. Pure and synchronous, so every fixture can be replayed in tests.

use std::collections::HashMap;

use kiln_agent::{
    AgentEvent, BlockKey, HookEdit, PermissionRequest, Remember, SessionCommand, SkillState,
    ToolCall, TurnOutcome,
};
use serde_json::Value;

use crate::{
    ask,
    control::{self, Asked},
    hooks, outgoing, skills, stream, subagent, system, usage,
    wire::{self, Line},
};

#[derive(Default)]
pub struct Translator {
    pending_permissions: HashMap<String, Pending>,
    /// Requests Kiln sent and awaits a reply to, by request id.
    asked: HashMap<String, Asked>,
    next_request: u64,
    /// Each skill's other names, from the last skills reply.
    skill_handles: HashMap<String, Value>,
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
            SessionCommand::SetModel(_)
            | SessionCommand::SetMode(_)
            | SessionCommand::SetEffort(_) => self.change(command),
            SessionCommand::RefreshContext
            | SessionCommand::RefreshUsage
            | SessionCommand::RefreshSkills
            | SessionCommand::RefreshHooks
            | SessionCommand::RefreshEffort => self.refresh(&command),
            // Not messages: the driver runs these edits (see `skill_edit`
            // and `hook_edit`).
            SessionCommand::SetSkillState { .. } | SessionCommand::EditHook(_) => None,
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

    /// A change to the model, mode or effort for the following turns.
    fn change(&mut self, command: SessionCommand) -> Option<Value> {
        match command {
            SessionCommand::SetModel(model) => {
                let id = self.ask("model", Asked::Model);
                Some(outgoing::set_model(&id, &model))
            }
            SessionCommand::SetMode(mode) => {
                let id = self.ask("mode", Asked::Mode);
                Some(outgoing::set_permission_mode(&id, control::mode_name(mode)))
            }
            SessionCommand::SetEffort(effort) => {
                let id = self.ask("effort", Asked::Effort(effort));
                Some(outgoing::set_effort(&id, control::effort_name(effort)))
            }
            _ => None,
        }
    }

    /// A request for something the agent reports only when asked.
    fn refresh(&mut self, command: &SessionCommand) -> Option<Value> {
        match command {
            SessionCommand::RefreshContext => {
                let id = self.ask("context", Asked::Context);
                Some(outgoing::get_context_usage(&id))
            }
            SessionCommand::RefreshUsage => {
                let id = self.ask("usage", Asked::Usage);
                Some(outgoing::get_usage(&id))
            }
            SessionCommand::RefreshSkills => {
                let id = self.ask("skills", Asked::Skills);
                Some(outgoing::get_skills_dialog(&id))
            }
            SessionCommand::RefreshHooks => {
                let id = self.ask("hooks", Asked::Hooks);
                Some(outgoing::get_hooks_listing(&id))
            }
            SessionCommand::RefreshEffort => {
                let id = self.ask("settings", Asked::Settings);
                Some(outgoing::get_settings(&id))
            }
            _ => None,
        }
    }

    /// What `edit-skill-overrides` reads to set the state of the skill
    /// `name`.
    #[must_use]
    pub fn skill_edit(&self, name: &str, state: SkillState) -> Value {
        skills::edit(name, state, self.skill_handles.get(name))
    }

    /// What `edit-hook` reads to make `edit`.
    #[must_use]
    pub fn hook_edit(&self, edit: &HookEdit) -> Value {
        hooks::edit(edit)
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
            Line::System(system) => system::events(system),
            Line::StreamEvent(event) => stream::event(event).into_iter().collect(),
            Line::Assistant(message) => assistant_events(message),
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
            Line::ConversationReset { trigger } => conversation_reset(&trigger),
            Line::TurnResult(result) => vec![AgentEvent::TurnEnded(outcome(result))],
            Line::Other => Vec::new(),
        }
    }

    fn control_response(&mut self, reply: wire::Reply) -> Option<AgentEvent> {
        match reply {
            wire::Reply::Success {
                request_id,
                response,
            } => match self.asked_by(&request_id)? {
                Asked::Skills => {
                    let listed = skills::from_reply(response)?;
                    self.skill_handles = listed.handles;
                    Some(AgentEvent::Skills(listed.skills))
                }
                asked => control::on_success(asked, response),
            },
            // Only changes the user asked for are worth showing as failed;
            // background refreshes run every turn and just keep the old
            // value.
            wire::Reply::Error { request_id, error } => match self.asked_by(&request_id)? {
                Asked::Model | Asked::Mode | Asked::Effort(_) => {
                    Some(AgentEvent::CommandFailed(error))
                }
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
        let questions = ask::questions(&tool_name, &input);
        Some(AgentEvent::PermissionRequest(PermissionRequest {
            id: request.request_id,
            tool: tool_name,
            input,
            description,
            remember,
            questions,
        }))
    }
}

/// Only `/clear` is known to empty the conversation. Another trigger keeps
/// the transcript: wrongly keeping history is harmless, wrongly wiping what
/// the user can see is not.
fn conversation_reset(trigger: &str) -> Vec<AgentEvent> {
    if trigger == "clear" {
        vec![AgentEvent::ConversationReset]
    } else {
        log::warn!("conversation reset by {trigger:?} kept the transcript");
        Vec::new()
    }
}

fn assistant_events(message: wire::Assistant) -> Vec<AgentEvent> {
    if message.local_command_run.is_some() {
        return command_output(message);
    }
    let written = message
        .parent_tool_use_id
        .as_deref()
        .map_or_default(|call| subagent::message(call, &message.message));
    written.into_iter().chain(tool_calls(message)).collect()
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
            wire::AssistantBlock::Text { .. } | wire::AssistantBlock::Other => None,
        })
        .collect()
}

/// A slash command's output arrives whole, as text blocks of a message no
/// model wrote; it is shown like a reply.
fn command_output(message: wire::Assistant) -> Vec<AgentEvent> {
    let message_id = message.message.id;
    (0..)
        .zip(message.message.content)
        .filter_map(|(index, block)| match block {
            wire::AssistantBlock::Text { text } if !text.is_empty() => {
                Some(AgentEvent::TextDelta {
                    block: BlockKey {
                        message_id: message_id.clone(),
                        index,
                    },
                    text,
                })
            }
            _ => None,
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
    if result.subtype == "success" && !result.is_error {
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
        let names = rules.iter().flat_map(rule_names).collect();
        return Some((Remember::Rules(names), Value::Array(rules)));
    }
    let accept_edits: Vec<Value> = of_type("setMode")
        .into_iter()
        .filter(|suggestion| suggestion.get("mode").and_then(Value::as_str) == Some("acceptEdits"))
        .collect();
    (!accept_edits.is_empty()).then_some((Remember::AcceptEdits, Value::Array(accept_edits)))
}

/// The rules an `addRules` suggestion adds, written as settings files hold
/// them: `Bash(npm test:*)`, or the bare tool name for a whole tool.
fn rule_names(suggestion: &Value) -> Vec<String> {
    let rules = suggestion.get("rules").and_then(Value::as_array);
    rules
        .into_iter()
        .flatten()
        .filter_map(|rule| {
            let tool = rule.get("toolName").and_then(Value::as_str)?;
            Some(match rule.get("ruleContent").and_then(Value::as_str) {
                Some(content) => format!("{tool}({content})"),
                None => tool.to_owned(),
            })
        })
        .collect()
}
