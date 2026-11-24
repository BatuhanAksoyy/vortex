//! The chat transcript as data: agent events fold into a list of entries.
//! Kept free of GPUI so the folding rules are unit-tested directly.

use std::collections::HashMap;

mod subagent;

pub use subagent::{Card, Step, Subagent};

use kiln_agent::{AgentEvent, BlockKey, PastMessage, PermissionRequest, ToolCall, TurnOutcome};

/// Stable identity of an entry, used for element ids and expansion state.
pub type EntryId = u64;

#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    User(String),
    Text {
        block: BlockKey,
        text: String,
    },
    Thinking {
        block: BlockKey,
        text: String,
    },
    Tool {
        call: ToolCall,
        result: Option<ToolResult>,
    },
    /// The call that started a subagent, once the agent reports it running.
    Subagent {
        call: ToolCall,
        result: Option<ToolResult>,
        agent: Box<Subagent>,
    },
    Notice {
        text: String,
        is_error: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolResult {
    pub output: String,
    pub is_error: bool,
}

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
    /// The row each tool call shows in; a subagent's calls show in its card.
    tools: HashMap<String, usize>,
    /// Permission requests awaiting an answer, oldest first. They are shown
    /// above the composer, not as transcript rows.
    pending: Vec<PermissionRequest>,
    status: Status,
    /// The last message was taken back before any reply, so the turn's end
    /// adds no "Interrupted" line under a message no longer shown.
    withdrawn: bool,
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
            Entry::User(text) => text.lines().map(str::trim).find(|line| !line.is_empty()),
            _ => None,
        })
    }

    /// The messages the user sent, newest first.
    pub fn sent(&self) -> impl Iterator<Item = &str> {
        self.entries
            .iter()
            .rev()
            .filter_map(|(_, entry)| match entry {
                Entry::User(text) => Some(text.as_str()),
                _ => None,
            })
    }

    /// Takes the last message back while nothing has come in reply; returns
    /// it, and the row it was in.
    pub fn withdraw(&mut self) -> Option<(usize, String)> {
        let Some((_, Entry::User(_))) = self.entries.last() else {
            return None;
        };
        let (_, entry) = self.entries.pop()?;
        let Entry::User(text) = entry else {
            return None;
        };
        self.withdrawn = true;
        Some((self.entries.len(), text))
    }

    /// Messages the user has sent; numbers the turns.
    pub fn turns(&self) -> usize {
        self.entries
            .iter()
            .filter(|(_, entry)| matches!(entry, Entry::User(_)))
            .count()
    }

    /// A line from Kiln itself, such as a change of account.
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
                PastMessage::User(text) => {
                    self.push(Entry::User(text));
                }
                PastMessage::Agent(event) => {
                    self.apply(event);
                }
            }
        }
        self.status = Status::Idle;
    }

    pub fn push_user(&mut self, text: String) -> Change {
        self.status = Status::Running;
        self.push(Entry::User(text))
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
            | AgentEvent::ToolCall(_)
            | AgentEvent::ToolResult { .. }
            | AgentEvent::Subagent(_) => self.apply_step(event),
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
            AgentEvent::TurnEnded(outcome) => self.end_turn(outcome),
            AgentEvent::Exited { error } => self.exit(error),
            _ => Change::default(),
        }
    }

    /// What the agent writes and does.
    fn apply_step(&mut self, event: AgentEvent) -> Change {
        match event {
            AgentEvent::TextDelta { block, text } => self.append_text(block, text, false),
            AgentEvent::ThinkingDelta { block, text } => self.append_text(block, text, true),
            AgentEvent::ToolCall(call) => self.push_tool(call),
            AgentEvent::ToolResult {
                id,
                output,
                is_error,
            } => self.finish_tool(&id, ToolResult { output, is_error }),
            AgentEvent::Subagent(event) => self.apply_subagent(event),
            _ => Change::default(),
        }
    }

    /// Forgets every entry; the turn under way, if any, goes on. Entry ids
    /// keep counting, so expansion state never carries over to a new entry.
    fn clear(&mut self) {
        self.entries.clear();
        self.tools.clear();
        self.pending.clear();
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
        if let Some((_, Entry::Text { block: last, text } | Entry::Thinking { block: last, text })) =
            self.entries.last_mut()
            && *last == block
        {
            text.push_str(&delta);
            return Change {
                updated: Some(self.entries.len() - 1),
                appended: 0,
            };
        }
        let text = delta;
        self.push(if thinking {
            Entry::Thinking { block, text }
        } else {
            Entry::Text { block, text }
        })
    }

    fn push_tool(&mut self, call: ToolCall) -> Change {
        if call.parent.is_some()
            && let Some(change) = self.push_step(call.clone())
        {
            return change;
        }
        self.tools.insert(call.id.clone(), self.entries.len());
        self.push(Entry::Tool { call, result: None })
    }

    fn finish_tool(&mut self, id: &str, result: ToolResult) -> Change {
        let Some(&index) = self.tools.get(id) else {
            return Change::default();
        };
        let entry = &mut self.entries[index].1;
        let changed = match entry {
            Entry::Tool { call, result: slot } if call.id == id => {
                *slot = Some(result);
                true
            }
            _ => Self::finish_step(entry, id, result),
        };
        Change {
            updated: changed.then_some(index),
            appended: 0,
        }
    }

    /// Unanswered requests die with their turn: the CLI no longer waits.
    fn end_turn(&mut self, outcome: TurnOutcome) -> Change {
        self.status = Status::Idle;
        self.pending.clear();
        let withdrawn = std::mem::take(&mut self.withdrawn);
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
        let (text, is_error) = match error {
            Some(error) => (format!("Session ended: {error}"), true),
            None => ("Session ended".to_owned(), false),
        };
        self.push(Entry::Notice { text, is_error })
    }
}

#[cfg(test)]
mod tests;
