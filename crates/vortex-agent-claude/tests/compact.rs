//! Replays a turn, then `/compact`: the CLI says it is compacting, how it
//! went, and where the compacted conversation starts.

// Inside a `cfg(test)` module so helpers may `unwrap`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod compact {
    use vortex_agent::{AgentEvent, Compaction};
    use vortex_agent_claude::Translator;

    use super::common::output_lines;

    #[test]
    fn compacting_is_reported_then_the_compaction_once() {
        let mut translator = Translator::default();
        let events: Vec<AgentEvent> = output_lines("compact")
            .iter()
            .flat_map(|line| translator.on_line(line))
            .collect();
        let started = events
            .iter()
            .position(|event| *event == AgentEvent::Compacting(true))
            .unwrap();
        let ended = events
            .iter()
            .position(|event| *event == AgentEvent::Compacting(false))
            .unwrap();
        let compaction = Compaction {
            automatic: false,
            before: 21_856,
            after: Some(6_807),
        };
        let compacted = events
            .iter()
            .position(|event| *event == AgentEvent::Compacted(compaction))
            .unwrap();
        assert!(started < ended && ended < compacted, "{events:#?}");
        assert!(!events.contains(&AgentEvent::CompactFailed));
        let texts = events
            .iter()
            .filter(|event| matches!(event, AgentEvent::TextDelta { .. }))
            .count();
        assert_eq!(texts, 1, "only the turn's reply, not the summary");
    }
}
