//! Replays a turn whose model handed a task to a subagent, which ran in the
//! background: its start, progress and report, and its own messages under
//! the call that started it.

// Inside a `cfg(test)` module so helpers may `unwrap`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod subagent {
    use std::time::Duration;

    use kiln_agent::{AgentEvent, SubagentChange, SubagentEnd, SubagentEvent, SubagentUsage};
    use kiln_agent_claude::Translator;

    use super::common::output_lines;

    fn events() -> Vec<AgentEvent> {
        let mut translator = Translator::default();
        output_lines("subagent")
            .iter()
            .flat_map(|line| translator.on_line(line))
            .collect()
    }

    fn agent_call(events: &[AgentEvent]) -> String {
        events
            .iter()
            .find_map(|event| match event {
                AgentEvent::ToolCall(call) if call.name == "Agent" => Some(call.id.clone()),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn the_subagent_reports_its_life_under_its_call() {
        let events = events();
        let call = agent_call(&events);
        let changes: Vec<&SubagentChange> = events
            .iter()
            .filter_map(|event| match event {
                AgentEvent::Subagent(SubagentEvent { call: of, change }) => {
                    assert_eq!(*of, call);
                    Some(change)
                }
                _ => None,
            })
            .collect();
        let started = SubagentChange::Started {
            description: "Read the note".into(),
            kind: Some("general-purpose".into()),
        };
        assert_eq!(changes.first(), Some(&&started));
        assert!(changes.contains(&&SubagentChange::Model("claude-haiku-5-5".into())));
        assert!(changes.contains(&&SubagentChange::Text("lantern".into())));
        let last_request = changes.iter().rev().find_map(|change| match change {
            SubagentChange::Request(tokens) => Some(*tokens),
            _ => None,
        });
        assert_eq!(
            last_request.map(|tokens| (tokens.cached, tokens.written, tokens.uncached)),
            Some((14_374, 2_948, 2))
        );
        assert!(changes.iter().any(|change| matches!(
            change,
            SubagentChange::Progress { activity, usage }
                if activity == "Reading note.txt" && usage.tool_uses == 1
        )));
        let finished = SubagentChange::Finished {
            end: SubagentEnd::Completed,
            summary: "lantern".into(),
            usage: Some(SubagentUsage {
                tokens: 17_343,
                tool_uses: 1,
                duration: Duration::from_millis(1_894),
            }),
        };
        assert_eq!(changes.last(), Some(&&finished));
    }

    #[test]
    fn the_subagents_tool_calls_name_it_as_their_parent() {
        let events = events();
        let call = agent_call(&events);
        let read = events
            .iter()
            .find_map(|event| match event {
                AgentEvent::ToolCall(read) if read.name == "Read" => Some(read),
                _ => None,
            })
            .unwrap();
        assert_eq!(read.parent.as_deref(), Some(call.as_str()));
        assert!(events.iter().any(|event| matches!(
            event,
            AgentEvent::ToolResult { id, .. } if *id == read.id
        )));
    }

    #[test]
    fn the_report_starts_a_turn_of_its_own() {
        let starts = events()
            .iter()
            .filter(|event| matches!(event, AgentEvent::Started { .. }))
            .count();
        let ends = events()
            .iter()
            .filter(|event| matches!(event, AgentEvent::TurnEnded(_)))
            .count();
        assert_eq!((starts, ends), (2, 2));
    }
}
