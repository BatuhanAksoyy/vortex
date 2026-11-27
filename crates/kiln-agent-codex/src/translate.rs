//! Turns the app-server's messages into `AgentEvent`s and Kiln's commands
//! into messages for it. Pure: the session driver does the I/O, so replay
//! tests feed captured lines straight in.

use std::collections::{HashMap, HashSet};

use kiln_agent::{
    Account, AgentEvent, ContextUsage, Effort, ModelOption, PermissionMode, SessionInfo,
    SpentTokens, TurnOutcome,
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::{
    protocol::{
        GetAccountRateLimitsResponse, GetAccountResponse, ModelListResponse, ThreadStartResponse,
    },
    report,
    rpc::{self, Incoming},
};

/// Where and how the thread starts.
#[derive(Clone, Debug, Default)]
pub struct Start {
    pub cwd: String,
    pub model: Option<String>,
    pub mode: PermissionMode,
    pub effort: Option<Effort>,
    /// A thread to continue instead of starting one.
    pub resume: Option<String>,
    /// Told to the model before the conversation, as Kiln's own guidance.
    pub instructions: Option<String>,
    /// Kiln's own MCP server, added to the user's.
    pub tools: Option<kiln_agent::ToolServer>,
}

/// What a message leads to: events for the chat and messages to send back.
#[derive(Debug, Default, PartialEq)]
pub struct Step {
    pub events: Vec<AgentEvent>,
    pub out: Vec<Value>,
    /// Set when the session cannot go on, with why.
    pub exit: Option<String>,
}

impl Step {
    fn event(event: AgentEvent) -> Self {
        Self {
            events: vec![event],
            ..Self::default()
        }
    }
}

/// What a request Kiln sent was for, to read its response.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Asked {
    Initialize,
    Thread,
    Models,
    Account,
    Limits,
    Turn,
    Other,
}

impl Asked {
    fn of(method: &str) -> Self {
        match method {
            "initialize" => Self::Initialize,
            "thread/start" | "thread/resume" => Self::Thread,
            "model/list" => Self::Models,
            "account/read" => Self::Account,
            "account/rateLimits/read" => Self::Limits,
            "turn/start" => Self::Turn,
            _ => Self::Other,
        }
    }
}

#[derive(Default)]
pub struct Translator {
    pub(crate) start: Start,
    next_id: i64,
    asked: HashMap<i64, Asked>,
    pub(crate) thread: Option<String>,
    pub(crate) turn: Option<String>,
    /// Approvals waiting for the user: the id the chat knows them by, and
    /// the server's request id with what it asks about.
    pub(crate) approvals: HashMap<String, (Value, crate::approval::Kind)>,
    /// The files each change item touches, for its approval card.
    pub(crate) changed_files: HashMap<String, Vec<String>>,
    /// The messages and thoughts whose words came as deltas.
    pub(crate) streamed: HashSet<String>,
    models: Option<Vec<ModelOption>>,
    account: Option<Account>,
    /// Whether the account was read, so `account` being `None` means none.
    account_read: bool,
    /// Whether `Ready` went out; later model lists are changes. The chat
    /// asks for the models after `Ready`, so a second one would loop.
    ready_sent: bool,
    pub(crate) context: Option<ContextUsage>,
    /// Messages sent before the thread started, sent once it has.
    pub(crate) queued: Vec<kiln_agent::Message>,
    pub(crate) spent: SpentTokens,
    pub(crate) subagents: crate::subagents::Subagents,
    pub(crate) thoughts: crate::thoughts::Thoughts,
}

impl Translator {
    #[must_use]
    pub fn new(start: Start) -> Self {
        Self {
            start,
            ..Self::default()
        }
    }

    /// The first message to send.
    pub fn begin(&mut self) -> Value {
        self.ask("initialize", crate::client::initialize_params())
    }

    pub(crate) fn ask(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.asked.insert(id, Asked::of(method));
        rpc::request(id, method, params)
    }

    /// Notes a request sent by someone else, so its response is read: a
    /// replay feeds the captured requests in this way.
    pub fn on_sent(&mut self, line: &str) {
        if let Some(Incoming::Request { id, method, .. }) = Incoming::parse(line)
            && let Some(id) = id.as_i64()
        {
            self.asked.insert(id, Asked::of(&method));
        }
    }

    pub fn on_line(&mut self, line: &str) -> Step {
        match Incoming::parse(line) {
            Some(Incoming::Response { id, result }) => {
                let asked = self.asked.remove(&id).unwrap_or(Asked::Other);
                self.on_response(asked, result)
            }
            Some(Incoming::Request { id, method, params }) => self.on_request(id, &method, params),
            Some(Incoming::Notification { method, params }) => {
                self.on_notification(&method, params)
            }
            None => Step::default(),
        }
    }

    fn on_response(&mut self, asked: Asked, result: Result<Value, String>) -> Step {
        let result = match (asked, result) {
            (Asked::Initialize | Asked::Thread, Err(error)) => {
                return Step {
                    exit: Some(error),
                    ..Step::default()
                };
            }
            (Asked::Turn, Err(error)) => {
                return Step::event(AgentEvent::TurnEnded(TurnOutcome::Failed(error)));
            }
            // Without them the chat still opens, with none to pick from.
            (Asked::Account | Asked::Models, Err(_)) => Value::Null,
            (_, Err(error)) => {
                log::warn!("a Codex request failed: {error}");
                return Step::default();
            }
            (_, Ok(result)) => result,
        };
        match asked {
            Asked::Initialize => self.initialized(),
            Asked::Thread => self.thread_started(&result),
            Asked::Models => self.models_listed(result),
            Asked::Account => {
                let account = decode::<GetAccountResponse>(result).and_then(|read| read.account);
                self.account = account.as_ref().and_then(report::account);
                self.account_read = true;
                self.ready()
            }
            Asked::Limits => {
                decode::<GetAccountRateLimitsResponse>(result).map_or_default(|read| {
                    Step::event(AgentEvent::Usage(report::usage(&read.rate_limits)))
                })
            }
            Asked::Turn | Asked::Other => Step::default(),
        }
    }

    fn initialized(&mut self) -> Step {
        let thread = self.open_thread();
        let out = vec![
            rpc::notification("initialized"),
            thread,
            self.ask("model/list", json!({})),
            self.ask("account/read", json!({ "refreshToken": false })),
            self.ask("account/rateLimits/read", Value::Null),
        ];
        Step {
            out,
            ..Step::default()
        }
    }

    fn open_thread(&mut self) -> Value {
        let (approval, sandbox) = crate::approval::policy(self.start.mode);
        let mut params = json!({
            "cwd": self.start.cwd,
            "approvalPolicy": approval,
            "sandbox": sandbox,
        });
        if let Some(model) = &self.start.model {
            params["model"] = json!(model);
        }
        let instructions = crate::tools::instructions(
            self.start.tools.as_ref(),
            self.start.instructions.as_deref(),
        );
        if let Some(instructions) = instructions {
            params["developerInstructions"] = json!(instructions);
        }
        if let Some(server) = &self.start.tools {
            params["config"] = crate::tools::config(server);
        }
        match self.start.resume.clone() {
            Some(thread) => {
                params["threadId"] = json!(thread);
                params["excludeTurns"] = json!(true);
                self.ask("thread/resume", params)
            }
            None => self.ask("thread/start", params),
        }
    }

    fn thread_started(&mut self, result: &Value) -> Step {
        let Some(started) = decode::<ThreadStartResponse>(result.clone()) else {
            return Step {
                exit: Some("Codex started no thread".to_owned()),
                ..Step::default()
            };
        };
        self.thread = Some(started.thread.id.clone());
        self.start.model = Some(started.model.clone());
        let mut step = Step::event(AgentEvent::Started {
            session_id: started.thread.id,
            model: started.model,
            cli_version: Some(started.thread.cli_version),
        });
        let effort = (self.start.effort)
            .or_else(|| started.reasoning_effort.as_deref().and_then(report::effort));
        self.start.effort = effort;
        step.events.push(AgentEvent::EffortChanged(effort));
        for message in std::mem::take(&mut self.queued) {
            step.out.extend(
                self.on_command(kiln_agent::SessionCommand::Send(message))
                    .out,
            );
        }
        step
    }

    /// Ready once both the models and the account are known.
    fn models_listed(&mut self, result: Value) -> Step {
        let listed = decode::<ModelListResponse>(result);
        let models = listed.map_or_default(|listed| report::models(&listed.data));
        if self.ready_sent {
            return Step::event(AgentEvent::ModelsChanged(models));
        }
        self.models = Some(models);
        self.ready()
    }

    fn ready(&mut self) -> Step {
        if self.ready_sent || !self.account_read {
            return Step::default();
        }
        let Some(models) = self.models.take() else {
            return Step::default();
        };
        self.ready_sent = true;
        Step::event(AgentEvent::Ready(SessionInfo {
            models,
            mode: self.start.mode,
            account: self.account.clone(),
            commands: Vec::new(),
        }))
    }
}

/// `None`, logged, for a message whose shape this version does not know.
pub(crate) fn decode<T: DeserializeOwned>(value: Value) -> Option<T> {
    serde_json::from_value(value)
        .inspect_err(|error| log::warn!("cannot read a Codex message: {error}"))
        .ok()
}
