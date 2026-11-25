//! Replays a subagent that started two subagents of its own: each reports
//! under the call that started it, which names the outer one as its parent.

// Inside a `cfg(test)` module so helpers may `unwrap`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod nested {
    use kiln_agent::{AgentEvent, SubagentChange, SubagentEnd, SubagentEvent, ToolCall};
    use kiln_agent_claude::Translator;

    use super::common::output_lines;

    fn events() -> Vec<AgentEvent> {
        let mut translator = Translator::default();
        output_lines("nested")
            .iter()
            .flat_map(|line| translator.on_line(line))
            .collect()
    }

    fn calls<'a>(events: &'a [AgentEvent], name: &str) -> Vec<(usize, &'a ToolCall)> {
        events
            .iter()
            .enumerate()
            .filter_map(|(at, event)| match event {
                AgentEvent::ToolCall(call) if call.name == name => Some((at, call)),
                _ => None,
            })
            .collect()
    }

    fn changes<'a>(events: &'a [AgentEvent], of: &str) -> Vec<(usize, &'a SubagentChange)> {
        events
            .iter()
            .enumerate()
            .filter_map(|(at, event)| match event {
                AgentEvent::Subagent(SubagentEvent { call, change }) if call == of => {
                    Some((at, change))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn each_subagent_lives_under_its_own_call() {
        let events = events();
        let agents = calls(&events, "Agent");
        let [(_, outer), inner @ ..] = agents.as_slice() else {
            panic!("no subagent");
        };
        assert_eq!(outer.parent, None);
        assert_eq!(inner.len(), 2);
        for (called, call) in inner {
            assert_eq!(call.parent.as_ref(), Some(&outer.id));
            let changes = changes(&events, &call.id);
            let (started, SubagentChange::Started { description, .. }) = changes[0] else {
                panic!("{changes:?}");
            };
            assert!(*called < started, "the call comes before its start");
            assert!(description.starts_with("Inner"), "{description}");
            let has = |wanted: &dyn Fn(&SubagentChange) -> bool| {
                changes.iter().any(|(_, change)| wanted(change))
            };
            assert!(has(&|change| matches!(change, SubagentChange::Model(_))));
            assert!(has(&|change| matches!(
                change,
                SubagentChange::Finished {
                    end: SubagentEnd::Completed,
                    usage: Some(_),
                    ..
                }
            )));
            let reads = calls(&events, "Read");
            let own = reads
                .iter()
                .filter(|(_, read)| read.parent.as_ref() == Some(&call.id));
            assert_eq!(own.count(), 1, "its call names it, not the outer one");
        }
    }
}
