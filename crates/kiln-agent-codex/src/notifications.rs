//! The app-server's notifications: a turn's progress, its items, and what
//! the account and context have spent.

use std::time::Duration;

use kiln_agent::{AgentEvent, BlockKey, TurnOutcome};
use serde_json::Value;

use crate::{
    items,
    protocol::{
        AccountRateLimitsUpdatedNotification, AgentMessageDeltaNotification,
        ItemCompletedNotification, ItemStartedNotification, ModelReroutedNotification,
        ReasoningSummaryTextDeltaNotification, ThreadItem, ThreadTokenUsageUpdatedNotification,
        TurnCompletedNotification, TurnStartedNotification, TurnStatus,
    },
    report,
    translate::{Step, Translator, decode},
};

/// Which thread a notification is about.
enum Route {
    Main,
    /// A subagent's, by its thread id.
    Child(String),
    /// One Kiln did not start, such as another client's.
    Other,
}

impl Translator {
    pub(crate) fn on_notification(&mut self, method: &str, params: Value) -> Step {
        let thread = params.get("threadId").and_then(Value::as_str);
        let events = match thread.map(|thread| self.route(thread)) {
            Some(Route::Child(thread)) => self.on_child(&thread, method, params),
            Some(Route::Other) => Vec::new(),
            Some(Route::Main) | None => self.on_main(method, params),
        };
        Step {
            events,
            ..Step::default()
        }
    }

    fn route(&self, thread: &str) -> Route {
        if self.subagents.is_child(thread) {
            Route::Child(thread.to_owned())
        } else if self.thread.as_deref().is_some_and(|main| main != thread) {
            Route::Other
        } else {
            Route::Main
        }
    }

    fn on_main(&mut self, method: &str, params: Value) -> Vec<AgentEvent> {
        match method {
            "turn/started" => {
                let started = decode::<TurnStartedNotification>(params);
                self.turn = started.map(|started| started.turn.id);
                Vec::new()
            }
            "turn/completed" => self.turn_completed(params),
            "item/started" => decode::<ItemStartedNotification>(params)
                .map_or_default(|started| self.item_started(&started.item)),
            "item/completed" => decode::<ItemCompletedNotification>(params)
                .map_or_default(|completed| self.item_completed(&completed.item)),
            "item/agentMessage/delta" => self.text_delta(params),
            "item/reasoning/summaryTextDelta" => self.thinking_delta(params),
            "thread/tokenUsage/updated" => self.token_usage(params),
            "account/rateLimits/updated" => decode::<AccountRateLimitsUpdatedNotification>(params)
                .map_or_default(|updated| {
                    vec![AgentEvent::Usage(report::usage(&updated.rate_limits))]
                }),
            "model/rerouted" => decode::<ModelReroutedNotification>(params)
                .map_or_default(|rerouted| vec![AgentEvent::ModelChanged(rerouted.to_model)]),
            _ => Vec::new(),
        }
    }

    fn item_started(&mut self, item: &ThreadItem) -> Vec<AgentEvent> {
        if let ThreadItem::SubAgentActivity(activity) = item {
            return self.subagent_activity(activity, false);
        }
        if let ThreadItem::FileChange(change) = item {
            let paths = change
                .changes
                .iter()
                .map(|file| file.path.clone())
                .collect();
            self.changed_files.insert(change.id.clone(), paths);
        }
        items::started(item)
    }

    fn item_completed(&mut self, item: &ThreadItem) -> Vec<AgentEvent> {
        match item {
            ThreadItem::AgentMessage(message) => {
                let block = BlockKey {
                    message_id: message.id.clone(),
                    index: 0,
                };
                self.rest(&message.id, &message.text)
                    .map(|text| AgentEvent::TextDelta { block, text })
                    .into_iter()
                    .collect()
            }
            ThreadItem::Reasoning(thought) => {
                let summary = thought.summary.as_deref().unwrap_or_default().join("\n\n");
                let block = BlockKey {
                    message_id: thought.id.clone(),
                    index: 0,
                };
                self.rest(&thought.id, &summary)
                    .map(|text| AgentEvent::ThinkingDelta { block, text })
                    .into_iter()
                    .collect()
            }
            ThreadItem::SubAgentActivity(activity) => self.subagent_activity(activity, true),
            _ => items::completed(item),
        }
    }

    /// The text of an item that came whole; `None` once its deltas spelled
    /// it out.
    fn rest(&mut self, id: &str, full: &str) -> Option<String> {
        let streamed = self.streamed.remove(id);
        (!streamed && !full.is_empty()).then(|| full.to_owned())
    }

    fn text_delta(&mut self, params: Value) -> Vec<AgentEvent> {
        let Some(delta) = decode::<AgentMessageDeltaNotification>(params) else {
            return Vec::new();
        };
        self.streamed.insert(delta.item_id.clone());
        let block = BlockKey {
            message_id: delta.item_id,
            index: 0,
        };
        vec![AgentEvent::TextDelta {
            block,
            text: delta.delta,
        }]
    }

    fn thinking_delta(&mut self, params: Value) -> Vec<AgentEvent> {
        let Some(delta) = decode::<ReasoningSummaryTextDeltaNotification>(params) else {
            return Vec::new();
        };
        self.streamed.insert(delta.item_id.clone());
        let block = BlockKey {
            message_id: delta.item_id,
            index: u64::try_from(delta.summary_index).unwrap_or_default(),
        };
        vec![AgentEvent::ThinkingDelta {
            block,
            text: delta.delta,
        }]
    }

    fn token_usage(&mut self, params: Value) -> Vec<AgentEvent> {
        let Some(updated) = decode::<ThreadTokenUsageUpdatedNotification>(params) else {
            return Vec::new();
        };
        self.spent = report::spent(&updated.token_usage);
        self.context = report::context(&updated.token_usage);
        self.context
            .clone()
            .map(AgentEvent::ContextUsage)
            .into_iter()
            .collect()
    }

    fn turn_completed(&mut self, params: Value) -> Vec<AgentEvent> {
        let Some(completed) = decode::<TurnCompletedNotification>(params) else {
            return Vec::new();
        };
        self.turn = None;
        let turn = completed.turn;
        let outcome = match turn.status {
            TurnStatus::Interrupted => TurnOutcome::Interrupted,
            TurnStatus::Failed => TurnOutcome::Failed(
                turn.error
                    .map_or_else(|| "The turn failed.".to_owned(), |error| error.message),
            ),
            _ => TurnOutcome::Completed,
        };
        let mut events = vec![AgentEvent::SpentTokens(self.spent)];
        if let Some(took) = turn.duration_ms.and_then(|ms| u64::try_from(ms).ok()) {
            events.push(AgentEvent::TurnTook {
                duration: Duration::from_millis(took),
                ended_at: None,
            });
        }
        events.push(AgentEvent::TurnEnded(outcome));
        events
    }
}
