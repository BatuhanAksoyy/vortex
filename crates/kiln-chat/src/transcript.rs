//! The chat transcript as data: agent events fold into a list of entries.
//! Kept free of GPUI so the folding rules are unit-tested directly.

use std::collections::HashMap;

use kiln_agent::{AgentEvent, BlockKey, Decision, PermissionRequest, ToolCall, TurnOutcome};
use serde_json::Value;

/// Stable identity of an entry, used for element ids. Entries are never
/// reordered, but ids stay meaningful even if that changes.
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
    Permission {
        request: PermissionRequest,
        decision: Option<Decision>,
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
    permissions: HashMap<String, usize>,
    status: Status,
    model: Option<String>,
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

    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    pub fn push_user(&mut self, text: String) -> Change {
        self.status = Status::Running;
        self.push(Entry::User(text))
    }

    /// Records the user's answer; `false` if the request is unknown or
    /// already answered.
    pub fn decide(&mut self, request_id: &str, decision: Decision) -> bool {
        let Some(&index) = self.permissions.get(request_id) else {
            return false;
        };
        match &mut self.entries[index].1 {
            Entry::Permission {
                decision: slot @ None,
                ..
            } => {
                *slot = Some(decision);
                true
            }
            _ => false,
        }
    }

    pub fn index_of_permission(&self, request_id: &str) -> Option<usize> {
        self.permissions.get(request_id).copied()
    }

    pub fn apply(&mut self, event: AgentEvent) -> Change {
        match event {
            AgentEvent::Started { model, .. } => {
                self.model = Some(model);
                Change::default()
            }
            AgentEvent::TextDelta { block, text } => self.append_text(block, text, false),
            AgentEvent::ThinkingDelta { block, text } => self.append_text(block, text, true),
            AgentEvent::ToolCall(call) => self.push_tool(call),
            AgentEvent::ToolResult {
                id,
                output,
                is_error,
            } => self.finish_tool(&id, ToolResult { output, is_error }),
            AgentEvent::PermissionRequest(request) => self.push_permission(request),
            AgentEvent::TurnEnded(outcome) => self.end_turn(outcome),
            AgentEvent::Exited { error } => self.exit(error),
        }
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

    fn push_permission(&mut self, request: PermissionRequest) -> Change {
        self.permissions
            .insert(request.id.clone(), self.entries.len());
        self.push(Entry::Permission {
            request,
            decision: None,
        })
    }

    fn end_turn(&mut self, outcome: TurnOutcome) -> Change {
        self.status = Status::Idle;
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
        let (text, is_error) = match error {
            Some(error) => (format!("Session ended: {error}"), true),
            None => ("Session ended".to_owned(), false),
        };
        self.push(Entry::Notice { text, is_error })
    }
}

/// One-line description of a tool call, like the CLI's `Bash(npm test)`.
pub fn tool_summary(name: &str, input: &Value) -> String {
    const KEYS: &[&str] = &[
        "command",
        "file_path",
        "path",
        "pattern",
        "url",
        "query",
        "description",
        "prompt",
    ];
    let detail = KEYS
        .iter()
        .find_map(|key| input.get(*key).and_then(Value::as_str))
        .unwrap_or_default();
    let first_line = detail.lines().next().unwrap_or_default();
    format!("{name}({first_line})")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn block(index: u64) -> BlockKey {
        BlockKey {
            message_id: "m1".into(),
            index,
        }
    }

    fn text(index: u64, text: &str) -> AgentEvent {
        AgentEvent::TextDelta {
            block: block(index),
            text: text.into(),
        }
    }

    fn call(id: &str) -> AgentEvent {
        AgentEvent::ToolCall(ToolCall {
            id: id.into(),
            parent: None,
            name: "Bash".into(),
            input: json!({ "command": "ls" }),
        })
    }

    #[test]
    fn deltas_of_one_block_extend_one_entry() {
        let mut transcript = Transcript::default();
        assert_eq!(transcript.apply(text(0, "Hel")).appended, 1);
        assert_eq!(transcript.apply(text(0, "lo")).updated, Some(0));
        assert_eq!(transcript.apply(text(1, "Next")).appended, 1);
        assert_eq!(transcript.len(), 2);
        assert!(
            matches!(transcript.entry(0), Some((_, Entry::Text { text, .. })) if text == "Hello")
        );
    }

    #[test]
    fn tool_result_updates_its_call_even_after_other_entries() {
        let mut transcript = Transcript::default();
        transcript.apply(call("t1"));
        transcript.apply(text(2, "working"));
        let change = transcript.apply(AgentEvent::ToolResult {
            id: "t1".into(),
            output: "a\nb".into(),
            is_error: false,
        });
        assert_eq!(
            change,
            Change {
                updated: Some(0),
                appended: 0
            }
        );
        assert!(
            matches!(transcript.entry(0), Some((_, Entry::Tool { result: Some(r), .. })) if r.output == "a\nb")
        );
    }

    #[test]
    fn a_permission_is_decided_once() {
        let mut transcript = Transcript::default();
        transcript.apply(AgentEvent::PermissionRequest(PermissionRequest {
            id: "p1".into(),
            tool: "Bash".into(),
            input: json!({}),
            description: None,
        }));
        assert!(transcript.decide("p1", Decision::Allow));
        assert!(!transcript.decide("p1", Decision::Deny));
        assert!(!transcript.decide("unknown", Decision::Deny));
    }

    #[test]
    fn status_follows_the_turn_and_the_process() {
        let mut transcript = Transcript::default();
        assert_eq!(transcript.status(), Status::Idle);
        transcript.push_user("hi".into());
        assert_eq!(transcript.status(), Status::Running);
        transcript.apply(AgentEvent::TurnEnded(TurnOutcome::Interrupted));
        assert_eq!(transcript.status(), Status::Idle);
        transcript.apply(AgentEvent::Exited {
            error: Some("boom".into()),
        });
        assert_eq!(transcript.status(), Status::Ended);
        assert!(
            matches!(transcript.entry(2), Some((_, Entry::Notice { is_error: true, text })) if text.contains("boom"))
        );
    }

    #[test]
    fn summaries_pick_the_meaningful_argument() {
        assert_eq!(
            tool_summary("Bash", &json!({ "command": "npm test\nmore" })),
            "Bash(npm test)"
        );
        assert_eq!(
            tool_summary("Read", &json!({ "file_path": "src/main.rs" })),
            "Read(src/main.rs)"
        );
        assert_eq!(
            tool_summary("TodoWrite", &json!({ "todos": [] })),
            "TodoWrite()"
        );
    }
}
