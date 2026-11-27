//! Codex's reasoning, as the chat's thoughts. A model often reasons in
//! several items in a row, each summed up in one short line; those join
//! one thought until another item starts. Codex says when each item started
//! and ended, which times the thought, and the next token report says how
//! many tokens the reasoning took.

use serde_json::Value;
use vortex_agent::{AgentEvent, BlockKey};

use crate::{
    protocol::{
        ItemStartedNotification, ReasoningSummaryTextDeltaNotification, ThreadItem,
        ThreadItemReasoning,
    },
    translate::{Translator, decode},
};

/// The thought under way: the reasoning items in a row so far.
#[derive(Debug)]
pub(crate) struct Thought {
    /// The first item's id, which the chat knows the thought by.
    id: String,
    started_ms: Option<i64>,
    /// The item and summary part last written, so a new one starts on a
    /// line of its own.
    part: Option<(String, i64)>,
}

#[derive(Debug, Default)]
pub(crate) struct Thoughts {
    current: Option<Thought>,
    /// A thought whose tokens the next report says.
    uncounted: Option<String>,
}

impl Translator {
    /// Any item but reasoning ends the thought under way.
    pub(crate) fn thought_item_started(&mut self, started: &ItemStartedNotification) {
        let ThreadItem::Reasoning(thought) = &started.item else {
            self.thoughts.current = None;
            return;
        };
        if self.thoughts.current.is_none() {
            self.thoughts.current = Some(Thought {
                id: thought.id.clone(),
                started_ms: Some(started.started_at_ms),
                part: None,
            });
        }
    }

    pub(crate) fn thinking_delta(&mut self, params: Value) -> Vec<AgentEvent> {
        let Some(delta) = decode::<ReasoningSummaryTextDeltaNotification>(params) else {
            return Vec::new();
        };
        self.streamed.insert(delta.item_id.clone());
        let part = (delta.item_id.clone(), delta.summary_index);
        self.thought_words(&delta.item_id, part, delta.delta)
    }

    /// The rest of a reasoning item that came whole, its time, and the
    /// thought to count once the tokens are reported.
    pub(crate) fn thought_completed(
        &mut self,
        thought: &ThreadItemReasoning,
        completed_ms: i64,
    ) -> Vec<AgentEvent> {
        let mut events = Vec::new();
        if !self.streamed.remove(&thought.id) {
            for (index, line) in thought.summary.iter().flatten().enumerate() {
                let part = (thought.id.clone(), i64::try_from(index).unwrap_or_default());
                events.extend(self.thought_words(&thought.id, part, line.clone()));
            }
        }
        let Some(current) = &self.thoughts.current else {
            return events;
        };
        if current.part.is_some() {
            self.thoughts.uncounted = Some(current.id.clone());
        }
        let took = current
            .started_ms
            .map(|start| completed_ms - start)
            .and_then(|took| u64::try_from(took).ok());
        if let (Some(took), Some(_)) = (took, &current.part) {
            events.push(AgentEvent::ThinkingTime {
                message_id: current.id.clone(),
                duration: std::time::Duration::from_millis(took),
            });
        }
        events
    }

    /// The tokens the reasoning of the last request took, for the thought
    /// it wrote.
    pub(crate) fn thought_tokens(&mut self, reasoning: i64) -> Option<AgentEvent> {
        let tokens = u64::try_from(reasoning).ok().filter(|tokens| *tokens > 0)?;
        let message_id = self.thoughts.uncounted.take()?;
        Some(AgentEvent::ThoughtTokens { message_id, tokens })
    }

    /// `text` of summary part `part`, written into the thought under way.
    fn thought_words(&mut self, item: &str, part: (String, i64), text: String) -> Vec<AgentEvent> {
        if text.is_empty() {
            return Vec::new();
        }
        let current = self.thoughts.current.get_or_insert_with(|| Thought {
            id: item.to_owned(),
            started_ms: None,
            part: None,
        });
        let new_part = current.part.as_ref().is_some_and(|last| *last != part);
        current.part = Some(part);
        let text = if new_part {
            format!("\n\n{text}")
        } else {
            text
        };
        let block = BlockKey {
            message_id: current.id.clone(),
            index: 0,
        };
        vec![AgentEvent::ThinkingDelta { block, text }]
    }
}
