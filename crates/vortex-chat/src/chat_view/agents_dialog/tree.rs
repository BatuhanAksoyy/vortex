//! The order the subagents view lists a conversation's subagents in.

use crate::transcript::{EntryId, Subagent, Subagents};

#[derive(Clone, Copy)]
pub struct Row<'a> {
    pub agent: &'a Subagent,
    /// How many subagents it is below.
    pub depth: usize,
}

/// Each subagent above those it started, in the order it started them; of
/// the main agent's, the newest first, after those with one running.
pub fn rows(agents: &Subagents) -> Vec<Row<'_>> {
    let idle = |agent: &Subagent| {
        agent.end.is_some() && agents.below(agent.id).iter().all(|it| it.end.is_some())
    };
    let mut outer: Vec<_> = agents.children(None).collect();
    outer.reverse();
    outer.sort_by_key(|agent| idle(agent));
    let mut rows = Vec::with_capacity(agents.len());
    for agent in outer {
        add(agent, 0, agents, &mut rows);
    }
    rows
}

fn add<'a>(agent: &'a Subagent, depth: usize, agents: &'a Subagents, rows: &mut Vec<Row<'a>>) {
    rows.push(Row { agent, depth });
    for child in agents.children(Some(agent.id)) {
        add(child, depth + 1, agents, rows);
    }
}

/// The subagent a step up or down the list from `shown`.
pub fn step(rows: &[Row], shown: Option<EntryId>, down: bool) -> Option<EntryId> {
    let at = rows
        .iter()
        .position(|row| Some(row.agent.id) == shown)
        .unwrap_or(0);
    let next = if down {
        (at + 1).min(rows.len().saturating_sub(1))
    } else {
        at.saturating_sub(1)
    };
    rows.get(next).map(|row| row.agent.id)
}

#[cfg(test)]
mod tests {
    use vortex_agent::{AgentEvent, SubagentChange, SubagentEnd, SubagentEvent, ToolCall};

    use super::*;
    use crate::transcript::Transcript;

    fn change(transcript: &mut Transcript, call: &str, change: SubagentChange) {
        let call = call.to_owned();
        transcript.apply(AgentEvent::Subagent(SubagentEvent { call, change }));
    }

    fn start(transcript: &mut Transcript, id: &str, parent: Option<&str>) {
        transcript.apply(AgentEvent::ToolCall(ToolCall {
            id: id.into(),
            parent: parent.map(Into::into),
            name: "Agent".into(),
            input: serde_json::json!({}),
        }));
        let started = SubagentChange::Started {
            description: id.into(),
            kind: None,
        };
        change(transcript, id, started);
    }

    fn end(transcript: &mut Transcript, id: &str) {
        let finished = SubagentChange::Finished {
            end: SubagentEnd::Completed,
            summary: String::new(),
            usage: None,
        };
        change(transcript, id, finished);
    }

    #[test]
    fn each_is_listed_under_the_one_that_started_it() {
        let mut transcript = Transcript::default();
        start(&mut transcript, "busy", None);
        start(&mut transcript, "busy-a", Some("busy"));
        start(&mut transcript, "done", None);
        start(&mut transcript, "busy-b", Some("busy"));
        start(&mut transcript, "deep", Some("busy-a"));
        for id in ["busy", "busy-a", "busy-b", "done"] {
            end(&mut transcript, id);
        }
        let rows = rows(transcript.subagents());
        let listed: Vec<_> = rows
            .iter()
            .map(|row| (row.agent.description.as_str(), row.depth))
            .collect();
        assert_eq!(
            listed,
            [
                ("busy", 0),
                ("busy-a", 1),
                ("deep", 2),
                ("busy-b", 1),
                ("done", 0)
            ],
            "one still runs below the older"
        );
        let first = rows[0].agent.id;
        assert_eq!(step(&rows, Some(first), true), Some(rows[1].agent.id));
        assert_eq!(step(&rows, Some(first), false), Some(first));
        assert_eq!(
            step(&rows, Some(rows[4].agent.id), true),
            Some(rows[4].agent.id)
        );
    }
}
