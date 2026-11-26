//! Kiln's session commands as app-server requests.

use base64::Engine as _;
use kiln_agent::{AgentEvent, Hooks, Message, SessionCommand};
use serde_json::{Value, json};

use crate::{
    approval, report,
    translate::{Step, Translator},
};

impl Translator {
    pub fn on_command(&mut self, command: SessionCommand) -> Step {
        match command {
            SessionCommand::Send(message) => self.send(&message),
            SessionCommand::Permission {
                request_id,
                decision,
            } => self.decide(&request_id, &decision).map_or_default(sending),
            SessionCommand::Interrupt => self.interrupt().map_or_default(sending),
            SessionCommand::RefreshUsage => {
                sending(self.ask("account/rateLimits/read", Value::Null))
            }
            SessionCommand::RefreshModels => sending(self.ask("model/list", json!({}))),
            SessionCommand::StopSubagent(call) => self.stop_subagent(&call).map_or_default(sending),
            command => self.answer(command).map_or_default(reporting),
        }
    }

    /// What a command Codex is not asked about comes back as: a choice kept
    /// for the next turn, or what Kiln knows already.
    fn answer(&mut self, command: SessionCommand) -> Option<AgentEvent> {
        let event = match command {
            SessionCommand::SetModel(model) => {
                self.start.model = Some(model.clone());
                AgentEvent::ModelChanged(model)
            }
            SessionCommand::SetMode(mode) => {
                self.start.mode = mode;
                AgentEvent::ModeChanged(mode)
            }
            SessionCommand::SetEffort(effort) => {
                self.start.effort = Some(effort);
                AgentEvent::EffortChanged(Some(effort))
            }
            SessionCommand::RefreshEffort => AgentEvent::EffortChanged(self.start.effort),
            SessionCommand::RefreshContext => AgentEvent::ContextUsage(self.context.clone()?),
            command => return unsupported(command),
        };
        Some(event)
    }

    fn send(&mut self, message: &Message) -> Step {
        let Some(thread) = self.thread.clone() else {
            self.queued.push(message.clone());
            return Step::default();
        };
        let input = input(message);
        // A message sent while a turn runs joins that turn.
        let out = if let Some(turn) = self.turn.clone() {
            self.ask(
                "turn/steer",
                json!({ "threadId": thread, "expectedTurnId": turn, "input": input }),
            )
        } else {
            let params = self.turn_params(&thread, &input);
            self.ask("turn/start", params)
        };
        sending(out)
    }

    fn turn_params(&self, thread: &str, input: &[Value]) -> Value {
        let mut params = json!({
            "threadId": thread,
            "input": input,
            "approvalPolicy": approval::policy(self.start.mode).0,
            "sandboxPolicy": approval::sandbox_policy(self.start.mode),
            "summary": "auto",
        });
        if let Some(model) = &self.start.model {
            params["model"] = json!(model);
        }
        if let Some(effort) = self.start.effort {
            params["effort"] = json!(report::effort_name(effort));
        }
        params
    }

    fn stop_subagent(&mut self, call: &str) -> Option<Value> {
        let (thread, turn) = self.subagents.running(call)?;
        Some(self.ask(
            "turn/interrupt",
            json!({ "threadId": thread, "turnId": turn }),
        ))
    }

    fn interrupt(&mut self) -> Option<Value> {
        let params = json!({ "threadId": self.thread.clone()?, "turnId": self.turn.clone()? });
        Some(self.ask("turn/interrupt", params))
    }
}

fn sending(out: Value) -> Step {
    Step {
        out: vec![out],
        ..Step::default()
    }
}

fn reporting(event: AgentEvent) -> Step {
    Step {
        events: vec![event],
        ..Step::default()
    }
}

/// The answer to a command for something Kiln does not support on Codex.
fn unsupported(command: SessionCommand) -> Option<AgentEvent> {
    match command {
        SessionCommand::RefreshSkills => Some(AgentEvent::Skills(Vec::new())),
        SessionCommand::RefreshHooks => Some(AgentEvent::Hooks(Hooks::default())),
        SessionCommand::SetSkillState { name, .. } => Some(AgentEvent::SkillStateFailed {
            name,
            error: "Kiln cannot change Codex's skills yet.".to_owned(),
        }),
        SessionCommand::EditHook(_) => Some(AgentEvent::HookEdited {
            error: Some("Kiln cannot change Codex's hooks yet.".to_owned()),
        }),
        // Codex has no recap.
        _ => None,
    }
}

/// The message as user input: its text, its pictures as data URLs, and its
/// files as mentions the model may read.
fn input(message: &Message) -> Vec<Value> {
    let mut input = vec![json!({ "type": "text", "text": message.text, "text_elements": [] })];
    for picture in &message.pictures {
        let data = base64::engine::general_purpose::STANDARD.encode(&picture.data);
        let url = format!("data:{};base64,{data}", picture.media_type);
        input.push(json!({ "type": "image", "url": url }));
    }
    for file in &message.files {
        let name = file.file_name().unwrap_or(file.as_os_str());
        input.push(json!({
            "type": "mention",
            "name": name.to_string_lossy(),
            "path": file.to_string_lossy(),
        }));
    }
    input
}
