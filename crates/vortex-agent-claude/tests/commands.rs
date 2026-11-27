//! Replays slash commands the CLI runs itself: the commands it offers, the
//! output of one, and `/clear` starting the conversation over.

// Inside a `cfg(test)` module so helpers may `unwrap`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod commands {
    use vortex_agent::{AgentEvent, SessionInfo, TurnOutcome};
    use vortex_agent_claude::Translator;

    use super::common::output_lines;

    fn translate() -> Vec<AgentEvent> {
        let mut translator = Translator::default();
        output_lines("commands")
            .iter()
            .flat_map(|line| translator.on_line(line))
            .collect()
    }

    #[test]
    fn the_commands_on_offer_come_with_their_hints_and_aliases() {
        let events = translate();
        let Some(AgentEvent::Ready(SessionInfo { commands, .. })) = events.first() else {
            panic!("{events:#?}");
        };
        let clear = commands.iter().find(|c| c.name == "clear").unwrap();
        assert_eq!(clear.aliases, ["reset", "new"]);
        assert_eq!(clear.argument_hint, "[name]");
        assert!(clear.description.starts_with("Start a new session"));
        let changed = events.iter().find_map(|e| match e {
            AgentEvent::CommandsChanged(commands) => Some(commands),
            _ => None,
        });
        assert_eq!(changed, Some(commands));
    }

    #[test]
    fn a_commands_output_is_shown_as_text() {
        let events = translate();
        let text: String = events
            .iter()
            .filter_map(|e| match e {
                AgentEvent::TextDelta { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(text.starts_with("Set effort level to low"), "{events:#?}");
    }

    #[test]
    fn clear_resets_the_conversation_and_completes() {
        let events = translate();
        let reset = events
            .iter()
            .position(|e| *e == AgentEvent::ConversationReset)
            .unwrap();
        let ends: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, AgentEvent::TurnEnded(_)))
            .collect();
        assert_eq!(ends, [&AgentEvent::TurnEnded(TurnOutcome::Completed); 2]);
        assert!(
            matches!(&events[reset + 1..], [AgentEvent::Started { .. }, ..]),
            "a new session starts after the reset: {events:#?}"
        );
    }

    #[test]
    fn a_reset_for_another_reason_keeps_the_transcript() {
        let line = r#"{"type":"conversation_reset","trigger":"compact","session_id":"s"}"#;
        assert_eq!(Translator::default().on_line(line), Vec::new());
    }
}
