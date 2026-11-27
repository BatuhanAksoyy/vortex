//! Replays a subagent sent a second task with `SendMessage` once it had
//! reported: the CLI runs it again as the same task, under the `SendMessage`
//! call, while its messages still name the call that started it.

// Inside a `cfg(test)` module so helpers may `unwrap`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod followup {
    use vortex_agent::{AgentEvent, SubagentChange, SubagentEnd, SubagentEvent};
    use vortex_agent_claude::Translator;

    use super::common::output_lines;

    fn events() -> Vec<AgentEvent> {
        let mut translator = Translator::default();
        output_lines("followup")
            .iter()
            .flat_map(|line| translator.on_line(line))
            .collect()
    }

    fn call(events: &[AgentEvent], name: &str) -> String {
        events
            .iter()
            .find_map(|event| match event {
                AgentEvent::ToolCall(call) if call.name == name => Some(call.id.clone()),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn the_resumed_subagent_reports_under_the_call_that_started_it() {
        let events = events();
        let agent = call(&events, "Agent");
        let send = call(&events, "SendMessage");
        let changes: Vec<&SubagentChange> = events
            .iter()
            .filter_map(|event| match event {
                AgentEvent::Subagent(SubagentEvent { call, change }) => {
                    assert_eq!(*call, agent, "not under {send}");
                    Some(change)
                }
                _ => None,
            })
            .collect();
        let started = changes
            .iter()
            .filter(|change| matches!(change, SubagentChange::Started { .. }))
            .count();
        assert_eq!(started, 1, "one card");
        let resumed = SubagentChange::Resumed {
            message: "Reply with the second word of note.txt.".into(),
        };
        let at = changes
            .iter()
            .position(|change| **change == resumed)
            .unwrap();
        let first_report = changes
            .iter()
            .position(|change| {
                matches!(change, SubagentChange::Finished { summary, .. } if summary == "lantern")
            })
            .unwrap();
        assert!(first_report < at, "resumed after its first report");
        assert!(changes.contains(&&SubagentChange::Text("moth".into())));
        assert!(matches!(
            changes.last(),
            Some(SubagentChange::Finished { end: SubagentEnd::Completed, summary, .. })
                if summary == "moth"
        ));
    }
}
