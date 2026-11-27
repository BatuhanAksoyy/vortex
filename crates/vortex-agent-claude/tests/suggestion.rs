//! Replays the captured prediction of the user's next message.

// Inside a `cfg(test)` module so helpers may `unwrap`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod suggestion {
    use vortex_agent::{AgentEvent, TurnOutcome};
    use vortex_agent_claude::Translator;

    use super::common::output_lines;

    #[test]
    fn the_next_message_is_predicted_after_the_turn() {
        let mut translator = Translator::default();
        let events: Vec<AgentEvent> = output_lines("suggestion")
            .iter()
            .flat_map(|line| translator.on_line(line))
            .collect();
        let suggested = events
            .iter()
            .position(|event| matches!(event, AgentEvent::PromptSuggestion(_)))
            .unwrap();
        assert_eq!(
            events[suggested],
            AgentEvent::PromptSuggestion("test it with mixed types".to_owned())
        );
        let ended = events
            .iter()
            .rposition(|event| *event == AgentEvent::TurnEnded(TurnOutcome::Completed))
            .unwrap();
        assert!(ended < suggested);
    }
}
