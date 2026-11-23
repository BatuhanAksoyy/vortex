//! Continuing a saved session with `--resume` (fixture `resume`): the CLI
//! keeps the session's id and its context.

// Inside a `cfg(test)` module so helpers may `unwrap`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod resume {
    use super::common::{fixture_path, output_lines};

    use kiln_agent::{AgentBackend as _, AgentEvent, SessionCommand, SessionSpec, TurnOutcome};
    use kiln_agent_claude::{ClaudeBackend, Translator};

    /// The id of the session the recording resumed, as the capture noted it.
    fn resumed_id() -> String {
        let text = std::fs::read_to_string(fixture_path("resume")).unwrap();
        text.lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .find(|record| record["dir"] == "note")
            .and_then(|record| record["msg"]["resumed"].as_str().map(str::to_owned))
            .unwrap()
    }

    fn run(resume: Option<String>) -> Vec<AgentEvent> {
        let backend = ClaudeBackend::new(env!("CARGO_BIN_EXE_fake-claude"));
        let kiln_agent::Session {
            events,
            commands,
            driver,
        } = backend
            .spawn(SessionSpec {
                cwd: std::env::temp_dir(),
                env: vec![(
                    "KILN_FAKE_CLAUDE_FIXTURE".into(),
                    fixture_path("resume").to_string_lossy().into_owned(),
                )],
                model: None,
                mode: None,
                effort: None,
                resume,
                fork: false,
            })
            .unwrap();
        let driver = std::thread::spawn(move || futures::executor::block_on(driver));
        commands
            .send_blocking(SessionCommand::Send("What word?".into()))
            .unwrap();
        let mut seen = Vec::new();
        while let Ok(event) = events.recv_blocking() {
            seen.push(event);
        }
        driver.join().unwrap();
        seen
    }

    #[test]
    fn a_resumed_session_keeps_its_id_and_its_context() {
        let mut translator = Translator::default();
        let events: Vec<AgentEvent> = output_lines("resume")
            .iter()
            .flat_map(|line| translator.on_line(line))
            .collect();
        let started = events.iter().find_map(|event| match event {
            AgentEvent::Started { session_id, .. } => Some(session_id.clone()),
            _ => None,
        });
        assert_eq!(started, Some(resumed_id()));
        let text: String = events
            .iter()
            .filter_map(|event| match event {
                AgentEvent::TextDelta { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(text.contains("marigold"), "{text}");
    }

    #[test]
    fn the_session_to_resume_goes_to_the_cli() {
        let events = run(Some(resumed_id()));
        assert!(events.contains(&AgentEvent::TurnEnded(TurnOutcome::Completed)));
        assert_eq!(events.last(), Some(&AgentEvent::Exited { error: None }));
    }

    #[test]
    fn a_new_session_does_not_resume() {
        let events = run(None);
        assert!(matches!(
            events.last(),
            Some(AgentEvent::Exited { error: Some(message) }) if message.contains("--resume")
        ));
    }
}
