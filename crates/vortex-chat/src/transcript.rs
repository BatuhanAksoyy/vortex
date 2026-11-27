//! The chat transcript as data: agent events fold into a list of entries.
//! Kept free of GPUI so the folding rules are unit-tested directly.

use std::collections::HashMap;

mod entry;
mod subagent;
mod thinking;
mod turn_end;

pub use entry::{Entry, ToolResult};
use subagent::{Holder, Place};
pub use subagent::{Step, StopAttempt, Subagent, Subagents};

use vortex_agent::{
    AgentEvent, BlockKey, Message, PastMessage, PermissionRequest, ToolCall, TurnOutcome,
};

/// Stable identity of an entry, used for element ids and expansion state.
pub type EntryId = u64;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    /// No agent process yet; the first message starts one.
    #[default]
    Idle,
    Running,
    /// The process ended; the next message starts a new one.
    Ended,
}

/// Which rows changed, so the view can remeasure or append only those.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Change {
    pub updated: Option<usize>,
    pub appended: usize,
}

#[derive(Default)]
pub struct Transcript {
    entries: Vec<(EntryId, Entry)>,
    next_id: EntryId,
    /// Where each tool call shows; a subagent's calls show in its card.
    tools: HashMap<String, Place>,
    agents: Subagents,
    /// Permission requests awaiting an answer, oldest first. They are shown
    /// above the composer, not as transcript rows.
    pending: Vec<PermissionRequest>,
    status: Status,
    /// The last message was taken back before any reply, so the turn's end
    /// adds no "Interrupted" line under a message no longer shown.
    withdrawn: bool,
    /// Whether the agent wrote or did anything since the turn began.
    answered: bool,
    /// Tokens of a thought reported before its first words.
    unclaimed_thinking: u64,
}

impl Transcript {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn entry(&self, index: usize) -> Option<(EntryId, &Entry)> {
        self.entries.get(index).map(|(id, entry)| (*id, entry))
    }

    pub fn status(&self) -> Status {
        self.status
    }

    /// The conversation's name: its first message's first line, so a pasted
    /// document does not fill the title bar.
    pub fn title(&self) -> Option<&str> {
        self.entries.iter().find_map(|(_, entry)| match entry {
            Entry::User(message) => message
                .text
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty()),
            _ => None,
        })
    }

    /// The messages the user sent, newest first.
    pub fn sent(&self) -> impl Iterator<Item = &str> {
        self.entries
            .iter()
            .rev()
            .filter_map(|(_, entry)| match entry {
                Entry::User(message) => Some(message.text.as_str()),
                _ => None,
            })
    }

    /// Takes the last message back while nothing has come in reply; returns
    /// it, and the row it was in.
    pub fn withdraw(&mut self) -> Option<(usize, Message)> {
        let Some((_, Entry::User(_))) = self.entries.last() else {
            return None;
        };
        let (_, entry) = self.entries.pop()?;
        let Entry::User(message) = entry else {
            return None;
        };
        // Taken back from behind a turn that answered already, it leaves
        // that turn to end as interrupted.
        self.withdrawn = !self.answered;
        Some((self.entries.len(), message))
    }

    /// Messages the user has sent; numbers the turns.
    pub fn turns(&self) -> usize {
        self.entries
            .iter()
            .filter(|(_, entry)| matches!(entry, Entry::User(_)))
            .count()
    }

    /// A line from Vortex itself, such as a change of account.
    pub fn push_notice(&mut self, text: String) -> Change {
        self.push(Entry::Notice {
            text,
            is_error: false,
        })
    }

    pub fn push_error(&mut self, text: String) -> Change {
        self.push(Entry::Notice {
            text,
            is_error: true,
        })
    }

    /// Starts over with a stored conversation; nothing is running.
    pub fn restore(&mut self, messages: Vec<PastMessage>) {
        self.clear();
        for message in messages {
            match message {
                PastMessage::User(message) => {
                    self.push(Entry::User(message));
                }
                PastMessage::Agent(event) => {
                    self.apply(event);
                }
            }
        }
        self.status = Status::Idle;
    }

    pub fn push_user(&mut self, message: Message) -> Change {
        // A message sent while a turn runs waits for it; it starts none.
        if self.status != Status::Running {
            self.withdrawn = false;
            self.answered = false;
        }
        self.status = Status::Running;
        self.push(Entry::User(message))
    }

    /// The permission request the user should answer next.
    pub fn pending_permission(&self) -> Option<&PermissionRequest> {
        self.pending.first()
    }

    /// Removes an answered request; `None` if it is unknown or was already
    /// answered.
    pub fn take_permission(&mut self, request_id: &str) -> Option<PermissionRequest> {
        let index = self
            .pending
            .iter()
            .position(|request| request.id == request_id)?;
        Some(self.pending.remove(index))
    }

    pub fn apply(&mut self, event: AgentEvent) -> Change {
        match event {
            AgentEvent::TextDelta { .. }
            | AgentEvent::ThinkingDelta { .. }
            | AgentEvent::ThinkingTokens(_)
            | AgentEvent::ThinkingTime { .. }
            | AgentEvent::ThoughtTokens { .. }
            | AgentEvent::ToolCall(_)
            | AgentEvent::ToolResult { .. }
            | AgentEvent::Subagent(_) => {
                self.answered = true;
                self.apply_step(event)
            }
            AgentEvent::PermissionRequest(request) => {
                self.pending.push(request);
                Change::default()
            }
            AgentEvent::CommandFailed(text) => self.push(Entry::Notice {
                text,
                is_error: true,
            }),
            AgentEvent::ConversationReset => {
                self.clear();
                Change::default()
            }
            // The agent went on by itself, e.g. to take a subagent's report
            // after the turn that started it ended. Only the main
            // conversation's requests are reported.
            AgentEvent::PromptTokens(_) if self.status == Status::Idle => {
                self.status = Status::Running;
                Change::default()
            }
            AgentEvent::TurnTook { .. } | AgentEvent::Recap(_) => self.sum_up(event),
            AgentEvent::TurnEnded(outcome) => self.end_turn(outcome),
            AgentEvent::Exited { error } => self.exit(error),
            _ => Change::default(),
        }
    }

    /// What the agent writes and does.
    fn apply_step(&mut self, event: AgentEvent) -> Change {
        match event {
            AgentEvent::TextDelta { block, text } => self.append_text(block, text, false),
            AgentEvent::ThinkingTokens(tokens) => self.count_thinking(tokens),
            AgentEvent::ThinkingTime {
                message_id,
                duration,
            } => self.time_thinking(&message_id, duration),
            AgentEvent::ThoughtTokens { message_id, tokens } => {
                self.thought_counted(&message_id, tokens)
            }
            AgentEvent::ThinkingDelta { block, text } => self.append_text(block, text, true),
            AgentEvent::ToolCall(call) => self.push_tool(call),
            AgentEvent::ToolResult {
                id,
                output,
                pictures,
                is_error,
                patch,
            } => self.finish_tool(
                &id,
                ToolResult {
                    output,
                    pictures,
                    is_error,
                    patch,
                },
            ),
            AgentEvent::Subagent(event) => self.apply_subagent(event),
            _ => Change::default(),
        }
    }

    /// Forgets every entry; the turn under way, if any, goes on. Entry ids
    /// keep counting, so expansion state never carries over to a new entry.
    fn clear(&mut self) {
        self.entries.clear();
        self.tools.clear();
        self.agents.clear();
        self.pending.clear();
        self.unclaimed_thinking = 0;
    }

    fn push(&mut self, entry: Entry) -> Change {
        self.next_id += 1;
        self.entries.push((self.next_id, entry));
        Change {
            updated: None,
            appended: 1,
        }
    }

    fn append_text(&mut self, block: BlockKey, delta: String, thinking: bool) -> Change {
        if let Some((
            _,
            Entry::Text { block: last, text }
            | Entry::Thinking {
                block: last, text, ..
            },
        )) = self.entries.last_mut()
            && *last == block
        {
            text.push_str(&delta);
            return Change {
                updated: Some(self.entries.len() - 1),
                appended: 0,
            };
        }
        let text = delta;
        let entry = if thinking {
            Entry::Thinking {
                block,
                text,
                tokens: std::mem::take(&mut self.unclaimed_thinking),
                took: None,
            }
        } else {
            Entry::Text { block, text }
        };
        self.push(entry)
    }

    fn push_tool(&mut self, call: ToolCall) -> Change {
        if call.parent.is_some()
            && let Some(change) = self.push_step(call.clone())
        {
            return change;
        }
        let place = Place {
            row: self.entries.len(),
            holder: Holder::Row,
        };
        self.tools.insert(call.id.clone(), place);
        self.push(Entry::Tool { call, result: None })
    }

    fn finish_tool(&mut self, id: &str, result: ToolResult) -> Change {
        let Some(&Place { row, holder }) = self.tools.get(id) else {
            return Change::default();
        };
        let changed = match holder {
            Holder::Row => self.finish_row(row, result),
            Holder::Step(agent) => self.finish_step(agent, id, result),
            Holder::Card(agent) => self.finish_call(agent, result),
        };
        Change {
            updated: changed.then_some(row),
            appended: 0,
        }
    }

    fn finish_row(&mut self, row: usize, result: ToolResult) -> bool {
        let Some((_, Entry::Tool { result: slot, .. })) = self.entries.get_mut(row) else {
            return false;
        };
        *slot = Some(result);
        true
    }

    /// The CLI waits on only for what subagents still running asked.
    fn end_turn(&mut self, outcome: TurnOutcome) -> Change {
        self.status = Status::Idle;
        self.unclaimed_thinking = 0;
        let withdrawn = std::mem::take(&mut self.withdrawn);
        if outcome != TurnOutcome::Completed {
            self.stop_foreground_subagents();
        }
        self.drop_requests(true);
        match outcome {
            TurnOutcome::Completed => Change::default(),
            TurnOutcome::Interrupted if withdrawn => Change::default(),
            TurnOutcome::Interrupted => self.push(Entry::Notice {
                text: "Interrupted".into(),
                is_error: false,
            }),
            TurnOutcome::Failed(message) => self.push(Entry::Notice {
                text: message,
                is_error: true,
            }),
        }
    }

    fn exit(&mut self, error: Option<String>) -> Change {
        self.status = Status::Ended;
        self.pending.clear();
        self.stop_all_subagents();
        let (text, is_error) = match error {
            Some(error) => (format!("Session ended: {error}"), true),
            None => ("Session ended".to_owned(), false),
        };
        self.push(Entry::Notice { text, is_error })
    }
}

#[cfg(test)]
mod tests;
