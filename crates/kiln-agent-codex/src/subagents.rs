//! Codex's subagents. Each runs on a thread of its own that streams on the
//! same connection; Kiln shows it as an `Agent` call of the main thread, with
//! the subagent's steps inside it, as it shows Claude's.

use std::collections::HashMap;

use kiln_agent::{AgentEvent, SubagentChange, SubagentEnd, SubagentEvent, ToolCall};
use serde_json::{Value, json};

use crate::{
    items,
    protocol::{
        ItemCompletedNotification, ItemStartedNotification, SubAgentActivityKind, ThreadItem,
        ThreadItemSubAgentActivity, TurnCompletedNotification, TurnStartedNotification, TurnStatus,
    },
    translate::{Translator, decode},
};

/// A subagent the main thread started, by its thread.
#[derive(Debug)]
pub(crate) struct Child {
    /// The id of the activity that started it, which the chat knows it by.
    call: String,
    /// Its running turn, to stop it.
    turn: Option<String>,
    /// What it last wrote, its report once it ends.
    said: String,
}

#[derive(Debug, Default)]
pub(crate) struct Subagents(HashMap<String, Child>);

impl Subagents {
    pub(crate) fn is_child(&self, thread: &str) -> bool {
        self.0.contains_key(thread)
    }

    /// The thread and turn to interrupt to stop the subagent the chat knows
    /// as `call`.
    pub(crate) fn running(&self, call: &str) -> Option<(String, String)> {
        let (thread, child) = self.0.iter().find(|(_, child)| child.call == call)?;
        Some((thread.clone(), child.turn.clone()?))
    }
}

impl Translator {
    /// A main-thread activity: a subagent started, or ended.
    pub(crate) fn subagent_activity(
        &mut self,
        activity: &ThreadItemSubAgentActivity,
        completed: bool,
    ) -> Vec<AgentEvent> {
        match (activity.kind, completed) {
            (SubAgentActivityKind::Started, false) => self.subagent_started(activity),
            (SubAgentActivityKind::Completed, true) => {
                self.subagent_ended(&activity.agent_thread_id, SubagentEnd::Completed)
            }
            (SubAgentActivityKind::Interrupted, true) => {
                self.subagent_ended(&activity.agent_thread_id, SubagentEnd::Stopped)
            }
            _ => Vec::new(),
        }
    }

    fn subagent_started(&mut self, activity: &ThreadItemSubAgentActivity) -> Vec<AgentEvent> {
        // The path names the task the model gave it, as `/root/heron_word`.
        let name = activity.agent_path.rsplit('/').next().unwrap_or_default();
        let description = name.replace('_', " ");
        let call = activity.id.clone();
        self.subagents.0.insert(
            activity.agent_thread_id.clone(),
            Child {
                call: call.clone(),
                turn: None,
                said: String::new(),
            },
        );
        let mut events = vec![
            AgentEvent::ToolCall(ToolCall {
                id: call.clone(),
                parent: None,
                name: "Agent".to_owned(),
                input: json!({ "description": description, "model": activity.model }),
            }),
            subagent(
                &call,
                SubagentChange::Started {
                    description,
                    kind: None,
                },
            ),
        ];
        if let Some(model) = &activity.model {
            events.push(subagent(&call, SubagentChange::Model(model.clone())));
        }
        events
    }

    fn subagent_ended(&mut self, thread: &str, end: SubagentEnd) -> Vec<AgentEvent> {
        let Some(child) = self.subagents.0.remove(thread) else {
            return Vec::new();
        };
        let summary = child.said;
        vec![
            subagent(
                &child.call,
                SubagentChange::Finished {
                    end,
                    summary: summary.clone(),
                    usage: None,
                },
            ),
            AgentEvent::ToolResult {
                id: child.call,
                output: summary,
                pictures: Vec::new(),
                is_error: end == SubagentEnd::Failed,
                patch: Vec::new(),
            },
        ]
    }

    /// A notification of subagent thread `thread`: its steps go inside its
    /// call; its turns are not the main thread's.
    pub(crate) fn on_child(
        &mut self,
        thread: &str,
        method: &str,
        params: Value,
    ) -> Vec<AgentEvent> {
        match method {
            "turn/started" => {
                let turn = decode::<TurnStartedNotification>(params).map(|it| it.turn.id);
                if let Some(child) = self.subagents.0.get_mut(thread) {
                    child.turn = turn;
                }
                Vec::new()
            }
            "turn/completed" => self.child_turn_completed(thread, params),
            "item/started" => decode::<ItemStartedNotification>(params)
                .map_or_default(|started| self.child_steps(thread, items::started(&started.item))),
            "item/completed" => decode::<ItemCompletedNotification>(params)
                .map_or_default(|completed| self.child_item_completed(thread, &completed.item)),
            _ => Vec::new(),
        }
    }

    fn child_item_completed(&mut self, thread: &str, item: &ThreadItem) -> Vec<AgentEvent> {
        let Some(child) = self.subagents.0.get_mut(thread) else {
            return Vec::new();
        };
        if let ThreadItem::AgentMessage(message) = item {
            child.said.clone_from(&message.text);
            return vec![subagent(
                &child.call,
                SubagentChange::Text(message.text.clone()),
            )];
        }
        self.child_steps(thread, items::completed(item))
    }

    /// A turn that failed or was stopped ends the subagent; a completed one
    /// waits for the main thread's activity saying so.
    fn child_turn_completed(&mut self, thread: &str, params: Value) -> Vec<AgentEvent> {
        let Some(completed) = decode::<TurnCompletedNotification>(params) else {
            return Vec::new();
        };
        if let Some(child) = self.subagents.0.get_mut(thread) {
            child.turn = None;
        }
        match completed.turn.status {
            TurnStatus::Failed => self.subagent_ended(thread, SubagentEnd::Failed),
            TurnStatus::Interrupted => self.subagent_ended(thread, SubagentEnd::Stopped),
            _ => Vec::new(),
        }
    }

    /// `events` of a subagent's item, its calls marked as the subagent's.
    fn child_steps(&self, thread: &str, mut events: Vec<AgentEvent>) -> Vec<AgentEvent> {
        let Some(child) = self.subagents.0.get(thread) else {
            return Vec::new();
        };
        for event in &mut events {
            if let AgentEvent::ToolCall(call) = event {
                call.parent = Some(child.call.clone());
            }
        }
        events
    }
}

fn subagent(call: &str, change: SubagentChange) -> AgentEvent {
    AgentEvent::Subagent(SubagentEvent {
        call: call.to_owned(),
        change,
    })
}
