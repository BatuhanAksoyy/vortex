//! Replays a turn, then a recap of the conversation.

// Inside a `cfg(test)` module so helpers may `unwrap`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod recap {
    use std::time::Duration;

    use vortex_agent::{AgentEvent, SessionCommand, TurnOutcome};
    use vortex_agent_claude::Translator;

    use super::common::output_lines;

    const ENDED: AgentEvent = AgentEvent::TurnEnded(TurnOutcome::Completed);

    /// The events of the turn and of the recap, which Vortex asks for when
    /// `asked`; otherwise the user typed the command.
    fn replay(asked: bool) -> (Vec<AgentEvent>, Vec<AgentEvent>) {
        let mut translator = Translator::default();
        let lines = output_lines("recap");
        let result = lines
            .iter()
            .position(|line| line.contains("\"num_turns\":1"))
            .unwrap();
        let turn: Vec<AgentEvent> = lines[..=result]
            .iter()
            .flat_map(|line| translator.on_line(line))
            .collect();
        assert!(turn.contains(&ENDED));
        if asked {
            let sent = translator.on_command(SessionCommand::Recap).unwrap();
            assert_eq!(sent["message"]["content"], "/recap");
        }
        let recap = lines[result + 1..]
            .iter()
            .flat_map(|line| translator.on_line(line))
            .collect();
        (turn, recap)
    }

    fn recaps(events: &[AgentEvent]) -> usize {
        let is_recap = |event: &&AgentEvent| matches!(event, AgentEvent::Recap(text) if text.starts_with("The overall goal"));
        events.iter().filter(is_recap).count()
    }

    #[test]
    fn a_turn_tells_how_long_it_took() {
        let (turn, _) = replay(true);
        assert!(turn.contains(&AgentEvent::TurnTook {
            duration: Duration::from_millis(1304),
            ended_at: None,
        }));
    }

    #[test]
    fn a_recap_vortex_asked_for_ends_no_turn() {
        let (_, recap) = replay(true);
        assert_eq!(recaps(&recap), 1, "{recap:#?}");
        assert!(!recap.contains(&ENDED));
    }

    #[test]
    fn a_recap_the_user_typed_ends_its_turn() {
        let (_, recap) = replay(false);
        assert_eq!(recaps(&recap), 1, "{recap:#?}");
        assert!(recap.contains(&ENDED));
    }

    #[test]
    fn none_is_asked_for_once_the_user_turned_them_off() {
        let mut translator = Translator::default();
        let asked = translator
            .on_command(SessionCommand::RefreshEffort)
            .unwrap();
        let reply = serde_json::json!({"type": "control_response", "response": {
            "subtype": "success", "request_id": asked["request_id"],
            "response": {"effective": {"awaySummaryEnabled": false}, "applied": {"effort": null}},
        }});
        translator.on_line(&reply.to_string());
        assert!(translator.on_command(SessionCommand::Recap).is_none());
    }
}
