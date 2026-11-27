//! A real child process, the fake app-server (`tests/support/fake_codex.rs`)
//! replaying a fixture, through the session driver.

#[cfg(test)]
mod process {
    use vortex_agent::{
        AgentBackend as _, AgentEvent, Decision, SessionCommand, SessionSpec, TurnOutcome,
    };
    use vortex_agent_codex::CodexBackend;

    /// Sends `message`, answers every approval with `decision`, and returns
    /// every event until the process is gone, ending it once the turn ends.
    fn run(name: &str, message: &str, decision: &Decision) -> Vec<AgentEvent> {
        let fixture = format!("{}/tests/fixtures/{name}.jsonl", env!("CARGO_MANIFEST_DIR"));
        let session = CodexBackend::new(env!("CARGO_BIN_EXE_fake-codex"))
            .spawn(SessionSpec {
                cwd: std::env::temp_dir(),
                env: vec![("VORTEX_FAKE_CODEX_FIXTURE".into(), fixture)],
                model: None,
                mode: None,
                effort: None,
                resume: None,
                fork: false,
                tools: None,
                handoff: None,
            })
            .unwrap();
        let driver = std::thread::spawn(move || futures::executor::block_on(session.driver));
        let mut commands = Some(session.commands);
        commands
            .as_ref()
            .unwrap()
            .send_blocking(SessionCommand::Send(message.into()))
            .unwrap();
        let mut seen = Vec::new();
        while let Ok(event) = session.events.recv_blocking() {
            match &event {
                AgentEvent::PermissionRequest(asked) => {
                    let answer = SessionCommand::Permission {
                        request_id: asked.id.clone(),
                        decision: decision.clone(),
                    };
                    commands.as_ref().unwrap().send_blocking(answer).unwrap();
                }
                AgentEvent::TurnEnded(_) => commands = None,
                _ => {}
            }
            seen.push(event);
        }
        driver.join().unwrap();
        seen
    }

    #[test]
    fn an_approved_command_round_trips_through_a_child_process() {
        let events = run("command", "Run echo vortex", &Decision::Allow);
        assert!(
            events.iter().any(|e| matches!(e, AgentEvent::Ready(_))),
            "{events:?}"
        );
        assert!(events.iter().any(
            |e| matches!(e, AgentEvent::ToolResult { output, is_error: false, .. } if output == "vortex\n")
        ), "{events:?}");
        assert!(events.contains(&AgentEvent::TurnEnded(TurnOutcome::Completed)));
        assert_eq!(events.last(), Some(&AgentEvent::Exited { error: None }));
    }

    #[test]
    fn a_declined_command_round_trips_through_a_child_process() {
        let events = run(
            "deny",
            "Run echo vortex",
            &Decision::Deny { feedback: None },
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, AgentEvent::ToolResult { is_error: true, .. })),
            "{events:?}"
        );
        assert!(events.contains(&AgentEvent::TurnEnded(TurnOutcome::Completed)));
    }
}
