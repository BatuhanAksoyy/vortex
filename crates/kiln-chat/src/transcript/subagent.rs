//! The subagents of a conversation: each call that started one turns into a
//! card that gathers its progress, report and own steps, which stay out of
//! the main timeline. One that a subagent started is a step of that one.

use std::collections::{BTreeMap, HashSet};

use kiln_agent::{
    PromptTokens, SubagentChange, SubagentEnd, SubagentEvent, SubagentUsage, ToolCall,
};

use super::{Change, Entry, EntryId, ToolResult, Transcript};

#[derive(Clone, Debug, PartialEq)]
pub struct Subagent {
    /// Its card's identity; a subagent's is above its parent's.
    pub id: EntryId,
    /// The subagent that started it; `None` for the main agent's.
    pub parent: Option<EntryId>,
    /// The timeline row its card, or the card around it, is in.
    pub row: usize,
    /// The call that started it, holding what it was asked.
    pub call: ToolCall,
    pub result: Option<ToolResult>,
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
    /// A subagent it started.
    Subagent(EntryId),
}

/// Where a tool call shows.
#[derive(Clone, Copy, Debug)]
pub(super) struct Place {
    pub row: usize,
    pub holder: Holder,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Holder {
    /// The row is the call's own.
    Row,
    /// A step of this subagent.
    Step(EntryId),
    /// The call started this subagent.
    Card(EntryId),
}

/// Every subagent, by id and so in the order they were started, parents
/// before the subagents they start.
#[derive(Debug, Default)]
pub struct Subagents(BTreeMap<EntryId, Subagent>);

impl Subagents {
    pub fn get(&self, id: EntryId) -> Option<&Subagent> {
        self.0.get(&id)
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &Subagent> {
        self.0.values()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn running(&self) -> usize {
        self.iter().filter(|agent| agent.end.is_none()).count()
    }

    /// Those subagent `id` started, `None` for the main agent's.
    pub fn children(&self, id: Option<EntryId>) -> impl Iterator<Item = &Subagent> {
        self.iter().filter(move |agent| agent.parent == id)
    }

    /// All below subagent `id`, at any depth.
    pub fn below(&self, id: EntryId) -> Vec<&Subagent> {
        let mut inside = HashSet::from([id]);
        self.iter()
            .filter(|agent| {
                let below = agent.parent.is_some_and(|parent| inside.contains(&parent));
                below && inside.insert(agent.id)
            })
            .collect()
    }

    /// Subagent `id` and those above it, the outermost first.
    pub fn path(&self, id: EntryId) -> Vec<&Subagent> {
        let above = |agent: &&Subagent| agent.parent.and_then(|parent| self.get(parent));
        let mut path: Vec<_> = std::iter::successors(self.get(id), above).collect();
        path.reverse();
        path
    }

    pub(super) fn clear(&mut self) {
        self.0.clear();
    }

    fn stop_all(&mut self) {
        for agent in self.0.values_mut() {
            agent.end.get_or_insert(SubagentEnd::Stopped);
        }
    }

    /// Ends the running ones whose whole chain of calls waits for the main
    /// agent's turn.
    fn stop_foreground(&mut self) {
        let mut waited_for = HashSet::new();
        for agent in self.0.values_mut() {
            let background =
                agent.call.input.get("run_in_background") == Some(&serde_json::Value::Bool(true));
            let detached = agent
                .parent
                .is_some_and(|parent| !waited_for.contains(&parent));
            if background || detached {
                continue;
            }
            waited_for.insert(agent.id);
            agent.end.get_or_insert(SubagentEnd::Stopped);
        }
    }
}

impl Transcript {
    pub fn subagents(&self) -> &Subagents {
        &self.agents
    }

    /// Marks stopped the subagents a turn that was interrupted or failed
    /// ran in the foreground: the CLI ends them with the turn but reports
    /// no end. Ones sent to the background run on and report their own.
    pub(super) fn stop_foreground_subagents(&mut self) {
        self.agents.stop_foreground();
    }

    /// Marks stopped every subagent still running, once the agent's process
    /// is gone and none can report an end.
    pub(super) fn stop_all_subagents(&mut self) {
        self.agents.stop_all();
    }

    pub(super) fn apply_subagent(&mut self, event: SubagentEvent) -> Change {
        let Some(&place) = self.tools.get(&event.call) else {
            return Change::default();
        };
        let changed = match (place.holder, event.change) {
            (Holder::Card(id), change) => self
                .agents
                .0
                .get_mut(&id)
                .is_some_and(|agent| agent.apply(change)),
            (holder, SubagentChange::Started { description, kind }) => {
                self.start(&event.call, place.row, holder, (description, kind))
            }
            // Without a start reported, its words are its caller's.
            (Holder::Step(id), SubagentChange::Text(text)) => self.add_step(id, Step::Text(text)),
            _ => false,
        };
        updated(changed, place.row)
    }

    /// Turns call `id`, in its row or among a subagent's steps, into a card.
    fn start(
        &mut self,
        id: &str,
        row: usize,
        holder: Holder,
        (description, kind): (String, Option<String>),
    ) -> bool {
        let Some((call, result)) = self.called(id, row, holder) else {
            return false;
        };
        let (card, parent) = match holder {
            Holder::Step(parent) => {
                self.next_id += 1;
                (self.next_id, Some(parent))
            }
            _ => (self.entries[row].0, None),
        };
        self.replace(id, row, holder, card);
        let holder = Holder::Card(card);
        self.tools.insert(id.to_owned(), Place { row, holder });
        let agent = Subagent {
            id: card,
            parent,
            row,
            call,
            result,
            description,
            kind,
            model: None,
            activity: None,
            usage: None,
            request: None,
            end: None,
            summary: None,
            steps: Vec::new(),
        };
        self.agents.0.insert(card, agent);
        true
    }

    fn called(
        &self,
        id: &str,
        row: usize,
        holder: Holder,
    ) -> Option<(ToolCall, Option<ToolResult>)> {
        if let Holder::Step(agent) = holder {
            let steps = &self.agents.get(agent)?.steps;
            return steps.iter().find_map(|step| match step {
                Step::Tool { call, result } if call.id == id => {
                    Some((call.clone(), result.clone()))
                }
                _ => None,
            });
        }
        match &self.entries.get(row)?.1 {
            Entry::Tool { call, result } => Some((call.clone(), result.clone())),
            _ => None,
        }
    }

    fn replace(&mut self, id: &str, row: usize, holder: Holder, card: EntryId) {
        let Holder::Step(agent) = holder else {
            self.entries[row].1 = Entry::Subagent;
            return;
        };
        let steps = self.agents.0.get_mut(&agent).map(|agent| &mut agent.steps);
        let call = steps
            .into_iter()
            .flatten()
            .find(|step| matches!(step, Step::Tool { call, .. } if call.id == id));
        if let Some(step) = call {
            *step = Step::Subagent(card);
        }
    }

    fn add_step(&mut self, id: EntryId, step: Step) -> bool {
        let agent = self.agents.0.get_mut(&id);
        agent.map(|agent| agent.steps.push(step)).is_some()
    }

    /// A tool call of a subagent goes under its card; `None` when its
    /// parent shows no card, so it is shown as a row of its own.
    pub(super) fn push_step(&mut self, call: ToolCall) -> Option<Change> {
        let parent = *self.tools.get(call.parent.as_ref()?)?;
        let (Holder::Card(agent) | Holder::Step(agent)) = parent.holder else {
            return None;
        };
        let place = Place {
            row: parent.row,
            holder: Holder::Step(agent),
        };
        self.tools.insert(call.id.clone(), place);
        let added = self.add_step(agent, Step::Tool { call, result: None });
        Some(updated(added, parent.row))
    }

    /// Fills in the result of a subagent's call.
    pub(super) fn finish_step(&mut self, agent: EntryId, id: &str, result: ToolResult) -> bool {
        let Some(agent) = self.agents.0.get_mut(&agent) else {
            return false;
        };
        let step = agent.steps.iter_mut().find_map(|step| match step {
            Step::Tool { call, result } if call.id == id => Some(result),
            _ => None,
        });
        step.map(|slot| *slot = Some(result)).is_some()
    }

    /// Fills in the result of the call that started a subagent.
    pub(super) fn finish_call(&mut self, agent: EntryId, result: ToolResult) -> bool {
        let Some(agent) = self.agents.0.get_mut(&agent) else {
            return false;
        };
        if result.is_error && agent.end.is_none() {
            agent.end = Some(SubagentEnd::Failed);
            agent.summary = Some(result.output.clone());
        }
        agent.result = Some(result);
        true
    }
}

impl Subagent {
    fn apply(&mut self, change: SubagentChange) -> bool {
        match change {
            SubagentChange::Model(model) if self.model.is_none() => self.model = Some(model),
            SubagentChange::Request(tokens) => self.request = Some(tokens),
            SubagentChange::Text(text) => self.steps.push(Step::Text(text)),
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

fn updated(changed: bool, index: usize) -> Change {
    Change {
        updated: changed.then_some(index),
        appended: 0,
    }
}

#[cfg(test)]
mod tests;
