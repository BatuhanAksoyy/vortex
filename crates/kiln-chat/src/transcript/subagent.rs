//! A subagent as a transcript row: the call that started it turns into a
//! card that gathers the subagent's progress, report and own steps, which
//! stay out of the main timeline.

use kiln_agent::{
    PromptTokens, SubagentChange, SubagentEnd, SubagentEvent, SubagentUsage, ToolCall,
};

use super::{Change, Entry, EntryId, ToolResult, Transcript};

#[derive(Clone, Debug, PartialEq)]
pub struct Subagent {
    pub description: String,
    pub kind: Option<String>,
    pub model: Option<String>,
    /// What it said it is doing, as of its last progress report.
    pub activity: Option<String>,
    pub usage: Option<SubagentUsage>,
    /// Its last request to the model, whose prompt fills its context.
    pub request: Option<PromptTokens>,
    /// `None` while it runs.
    pub end: Option<SubagentEnd>,
    pub summary: Option<String>,
    pub steps: Vec<Step>,
}

/// A subagent card with its place on the timeline.
#[derive(Clone, Copy)]
pub struct Card<'a> {
    pub index: usize,
    pub id: EntryId,
    /// The call that started it, holding what it was asked.
    pub call: &'a ToolCall,
    pub agent: &'a Subagent,
}

/// Something the subagent did, in order.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Tool {
        call: ToolCall,
        result: Option<ToolResult>,
    },
    Text(String),
    /// A message it was sent after it started, which it runs again to answer.
    Message(String),
}

impl Transcript {
    /// The subagent cards, in timeline order.
    pub fn subagents(&self) -> impl Iterator<Item = Card<'_>> {
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(index, (id, entry))| match entry {
                Entry::Subagent { call, agent, .. } => Some(Card {
                    index,
                    id: *id,
                    call,
                    agent: agent.as_ref(),
                }),
                _ => None,
            })
    }

    /// Marks stopped the subagents a turn that was interrupted or failed
    /// ran in the foreground: the CLI ends them with the turn but reports
    /// no end. Ones sent to the background run on and report their own.
    pub(super) fn stop_foreground_subagents(&mut self) {
        self.stop_subagents(|call| {
            call.input.get("run_in_background") != Some(&serde_json::Value::Bool(true))
        });
    }

    /// Marks stopped every subagent still running, once the agent's process
    /// is gone and none can report an end.
    pub(super) fn stop_all_subagents(&mut self) {
        self.stop_subagents(|_| true);
    }

    fn stop_subagents(&mut self, started_by: impl Fn(&ToolCall) -> bool) {
        for (_, entry) in &mut self.entries {
            if let Entry::Subagent { call, agent, .. } = entry
                && agent.end.is_none()
                && started_by(call)
            {
                agent.end = Some(SubagentEnd::Stopped);
            }
        }
    }

    pub(super) fn apply_subagent(&mut self, event: SubagentEvent) -> Change {
        let Some(&index) = self.tools.get(&event.call) else {
            return Change::default();
        };
        let entry = &mut self.entries[index].1;
        let changed = match (entry, event.change) {
            (entry @ Entry::Tool { .. }, SubagentChange::Started { description, kind }) => {
                start(entry, &event.call, description, kind)
            }
            // A nested subagent's text joins the steps of the card it runs
            // under; its other reports are its own card's business.
            (Entry::Subagent { agent, .. }, SubagentChange::Text(text)) => {
                agent.steps.push(Step::Text(text));
                true
            }
            (Entry::Subagent { call, agent, .. }, change) if call.id == event.call => {
                agent.apply(change)
            }
            _ => false,
        };
        updated(changed, index)
    }

    /// A tool call of a subagent goes under its card; `None` when its
    /// parent shows no card, so it is shown as a row of its own.
    pub(super) fn push_step(&mut self, call: ToolCall) -> Option<Change> {
        let parent = call.parent.as_ref()?;
        let index = *self.tools.get(parent)?;
        let Entry::Subagent { agent, .. } = &mut self.entries[index].1 else {
            return None;
        };
        self.tools.insert(call.id.clone(), index);
        agent.steps.push(Step::Tool { call, result: None });
        Some(updated(true, index))
    }

    /// Fills in the result of the card's own call or of one of its steps.
    pub(super) fn finish_step(entry: &mut Entry, id: &str, result: ToolResult) -> bool {
        let Entry::Subagent {
            call,
            result: own,
            agent,
        } = entry
        else {
            return false;
        };
        if call.id == id {
            if result.is_error && agent.end.is_none() {
                agent.end = Some(SubagentEnd::Failed);
                agent.summary = Some(result.output.clone());
            }
            *own = Some(result);
            return true;
        }
        let step = agent.steps.iter_mut().find_map(|step| match step {
            Step::Tool { call, result } if call.id == id => Some(result),
            _ => None,
        });
        step.map(|slot| *slot = Some(result)).is_some()
    }
}

impl Subagent {
    fn apply(&mut self, change: SubagentChange) -> bool {
        match change {
            SubagentChange::Model(model) if self.model.is_none() => self.model = Some(model),
            SubagentChange::Request(tokens) => self.request = Some(tokens),
            SubagentChange::Resumed { message } => {
                self.steps.push(Step::Message(message));
                self.end = None;
                self.summary = None;
                self.activity = None;
            }
            SubagentChange::Progress { activity, usage } if self.end.is_none() => {
                self.activity = Some(activity).filter(|activity| !activity.is_empty());
                self.usage = Some(usage);
            }
            SubagentChange::Finished {
                end,
                summary,
                usage,
            } => {
                self.end = Some(end);
                self.summary = Some(summary).filter(|summary| !summary.is_empty());
                self.usage = usage.or(self.usage);
            }
            _ => return false,
        }
        true
    }
}

/// Turns the row of call `id` into its subagent's card.
fn start(entry: &mut Entry, id: &str, description: String, kind: Option<String>) -> bool {
    let Entry::Tool { call, result } = entry else {
        return false;
    };
    if call.id != id {
        return false;
    }
    *entry = Entry::Subagent {
        call: call.clone(),
        result: result.take(),
        agent: Box::new(Subagent {
            description,
            kind,
            model: None,
            activity: None,
            usage: None,
            request: None,
            end: None,
            summary: None,
            steps: Vec::new(),
        }),
    };
    true
}

fn updated(changed: bool, index: usize) -> Change {
    Change {
        updated: changed.then_some(index),
        appended: 0,
    }
}
