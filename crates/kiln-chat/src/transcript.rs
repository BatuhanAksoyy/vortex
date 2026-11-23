//! The chat transcript as data: agent events fold into a list of entries.
//! Kept free of GPUI so the folding rules are unit-tested directly.

use std::collections::HashMap;

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
    tools: HashMap<String, usize>,
    /// Permission requests awaiting an answer, oldest first. They are shown
    /// above the composer, not as transcript rows.
    pending: Vec<PermissionRequest>,
    status: Status,
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
            AgentEvent::TextDelta { block, text } => self.append_text(block, text, false),
            AgentEvent::ThinkingDelta { block, text } => self.append_text(block, text, true),
            AgentEvent::ToolCall(call) => self.push_tool(call),
            AgentEvent::ToolResult {
                id,
                output,
                is_error,
            } => self.finish_tool(&id, ToolResult { output, is_error }),
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
            AgentEvent::TurnEnded(outcome) => self.end_turn(outcome),
            AgentEvent::Exited { error } => self.exit(error),
            AgentEvent::Started { .. }
            | AgentEvent::Ready(_)
            | AgentEvent::ModelChanged(_)
            | AgentEvent::ModeChanged(_)
            | AgentEvent::ContextUsage(_)
            | AgentEvent::Usage(_)
            | AgentEvent::CommandsChanged(_)
            | AgentEvent::Skills(_)
            | AgentEvent::SkillStateFailed { .. } => Change::default(),
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
        self.tools.insert(call.id.clone(), self.entries.len());
        self.push(Entry::Tool { call, result: None })
    }

    fn finish_tool(&mut self, id: &str, result: ToolResult) -> Change {
        let Some(&index) = self.tools.get(id) else {
            return Change::default();
        };
        if let Entry::Tool { result: slot, .. } = &mut self.entries[index].1 {
            *slot = Some(result);
        }
        Change {
            updated: Some(index),
            appended: 0,
        }
    }

    /// Unanswered requests die with their turn: the CLI no longer waits.
    fn end_turn(&mut self, outcome: TurnOutcome) -> Change {
        self.status = Status::Idle;
        self.pending.clear();
        match outcome {
            TurnOutcome::Completed => Change::default(),
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
