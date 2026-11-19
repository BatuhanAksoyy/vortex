//! Replays captured Claude Code sessions (see `tests/fixtures/capture.py`).

// Inside a `cfg(test)` module so helpers may `unwrap`, like unit tests.
#[cfg(test)]
mod replay {
    use std::path::{Path, PathBuf};

    use kiln_agent::{
        AgentBackend as _, AgentEvent, Decision, SessionCommand, SessionSpec, TurnOutcome,
    };
    use kiln_agent_claude::{ClaudeBackend, Translator};

    const FIXTURES: [&str; 3] = ["allow", "deny", "interrupt"];

    fn fixture_path(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(format!("{name}.jsonl"))
    }

    fn output_lines(name: &str) -> Vec<String> {
        let text = std::fs::read_to_string(fixture_path(name)).unwrap();
        text.lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .filter(|record| record["dir"] == "out")
            .map(|record| record["msg"].to_string())
            .collect()
    }

    fn translate(name: &str) -> Vec<AgentEvent> {
        let mut translator = Translator::default();
        output_lines(name)
            .iter()
            .flat_map(|line| translator.on_line(line))
            .collect()
    }

    fn text_of(events: &[AgentEvent]) -> String {
        events
            .iter()
            .filter_map(|event| match event {
                AgentEvent::TextDelta { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn fixtures_hold_no_personal_data() {
        for name in FIXTURES {
            let text = std::fs::read_to_string(fixture_path(name)).unwrap();
            for needle in ["/home/", "/Users/", "/tmp/", "@"] {
                assert!(!text.contains(needle), "{name} contains {needle:?}");
            }
        }
    }

    #[test]
    fn every_output_line_decodes() {
        for name in FIXTURES {
            for line in output_lines(name) {
                assert!(Translator::decode(&line).is_ok(), "{name}: {line}");
            }
        }
    }

    #[test]
    fn allowed_tool_call_runs_and_turn_completes() {
        let events = translate("allow");
        assert!(
            matches!(&events[0], AgentEvent::Started { model, .. } if model == "claude-haiku-5-5")
        );
        let call = events
            .iter()
            .position(|e| matches!(e, AgentEvent::ToolCall(c) if c.name == "Bash"));
        let ask = events
            .iter()
            .position(|e| matches!(e, AgentEvent::PermissionRequest(r) if r.tool == "Bash"));
        let result = events.iter().position(|e| {
            matches!(
                e,
                AgentEvent::ToolResult {
                    is_error: false,
                    ..
                }
            )
        });
        assert!(call < ask && ask < result && call.is_some(), "{events:#?}");
        assert_eq!(text_of(&events), "done");
        assert_eq!(
            events.last(),
            Some(&AgentEvent::TurnEnded(TurnOutcome::Completed))
        );
    }

    #[test]
    fn denied_tool_call_reports_an_error_result() {
        let events = translate("deny");
        assert!(events.iter().any(|e| matches!(
            e,
            AgentEvent::ToolResult { is_error: true, output, .. } if output.contains("denied")
        )));
        assert_eq!(
            events.last(),
            Some(&AgentEvent::TurnEnded(TurnOutcome::Completed))
        );
    }

    #[test]
    fn interrupted_turn_is_reported_as_interrupted() {
        let events = translate("interrupt");
        assert!(text_of(&events).starts_with("1\n2\n3"));
        assert_eq!(
            events.last(),
            Some(&AgentEvent::TurnEnded(TurnOutcome::Interrupted))
        );
    }

    #[test]
    fn permission_is_answered_once_with_the_original_input() {
        let mut translator = Translator::default();
        let request_id = output_lines("allow")
            .iter()
            .flat_map(|line| translator.on_line(line))
            .find_map(|event| match event {
                AgentEvent::PermissionRequest(request) => Some(request.id),
                _ => None,
            })
            .unwrap();
        let answer = |translator: &mut Translator| {
            translator.on_command(SessionCommand::Permission {
                request_id: request_id.clone(),
                decision: Decision::Allow,
            })
        };
        let response = answer(&mut translator).unwrap();
        let body = &response["response"]["response"];
        assert_eq!(body["behavior"], "allow");
        assert_eq!(body["updatedInput"]["command"], "touch kiln-fixture.txt");
        assert_eq!(answer(&mut translator), None);
    }

    // Process-level tests: a real child process (the fake CLI) through the
    // session driver.

    fn start(name: &str) -> kiln_agent::Session {
        let backend = ClaudeBackend::new(env!("CARGO_BIN_EXE_fake-claude"));
        backend
            .spawn(SessionSpec {
                cwd: std::env::temp_dir(),
                env: vec![(
                    "KILN_FAKE_CLAUDE_FIXTURE".into(),
                    fixture_path(name).to_string_lossy().into_owned(),
                )],
            })
            .unwrap()
    }

    /// Runs the session driver on its own thread and feeds every event to
    /// `react` until the process exits; returns all events.
    fn run(
        name: &str,
        first_message: &str,
        mut react: impl FnMut(&AgentEvent) -> Option<SessionCommand>,
    ) -> Vec<AgentEvent> {
        let kiln_agent::Session {
            events,
            commands,
            driver,
        } = start(name);
        let driver = std::thread::spawn(move || futures::executor::block_on(driver));
        commands
            .send_blocking(SessionCommand::Send(first_message.into()))
            .unwrap();
        let mut seen = Vec::new();
        while let Ok(event) = events.recv_blocking() {
            if let Some(command) = react(&event) {
                let _ = commands.send_blocking(command);
            }
            seen.push(event);
        }
        driver.join().unwrap();
        seen
    }

    #[test]
    fn allow_round_trip_through_a_child_process() {
        let events = run("allow", "touch it", |event| match event {
            AgentEvent::PermissionRequest(request) => Some(SessionCommand::Permission {
                request_id: request.id.clone(),
                decision: Decision::Allow,
            }),
            _ => None,
        });
        assert!(events.contains(&AgentEvent::TurnEnded(TurnOutcome::Completed)));
        assert_eq!(events.last(), Some(&AgentEvent::Exited { error: None }));
    }

    #[test]
    fn deny_round_trip_through_a_child_process() {
        let events = run("deny", "touch it", |event| match event {
            AgentEvent::PermissionRequest(request) => Some(SessionCommand::Permission {
                request_id: request.id.clone(),
                decision: Decision::Deny,
            }),
            _ => None,
        });
        assert_eq!(events.last(), Some(&AgentEvent::Exited { error: None }));
    }

    #[test]
    fn interrupt_round_trip_through_a_child_process() {
        let mut interrupted = false;
        let events = run("interrupt", "count", |event| match event {
            AgentEvent::TextDelta { .. } if !interrupted => {
                interrupted = true;
                Some(SessionCommand::Interrupt)
            }
            _ => None,
        });
        assert!(events.contains(&AgentEvent::TurnEnded(TurnOutcome::Interrupted)));
        assert_eq!(events.last(), Some(&AgentEvent::Exited { error: None }));
    }

    #[test]
    fn wrong_answer_makes_the_fake_fail() {
        let events = run("allow", "touch it", |event| match event {
            AgentEvent::PermissionRequest(request) => Some(SessionCommand::Permission {
                request_id: request.id.clone(),
                decision: Decision::Deny,
            }),
            _ => None,
        });
        assert!(matches!(
            events.last(),
            Some(AgentEvent::Exited { error: Some(message) }) if message.contains("expected")
        ));
    }
}

/// Talks to the installed `claude` CLI. Run by hand after protocol changes:
/// `cargo test -p kiln-agent-claude --test replay -- --ignored`.
#[cfg(test)]
mod live {
    use kiln_agent::{
        AgentBackend as _, AgentEvent, Decision, SessionCommand, SessionSpec, TurnOutcome,
    };
    use kiln_agent_claude::ClaudeBackend;

    #[test]
    #[ignore = "spends subscription usage and needs a logged-in `claude`"]
    fn live_claude_allows_a_tool_call() {
        let dir = std::env::temp_dir().join(format!("kiln-live-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let spec = SessionSpec {
            cwd: dir.clone(),
            env: vec![("ANTHROPIC_MODEL".into(), "haiku".into())],
        };
        let kiln_agent::Session {
            events,
            commands,
            driver,
        } = ClaudeBackend::new("claude").spawn(spec).unwrap();
        let driver = std::thread::spawn(move || futures::executor::block_on(driver));
        let prompt = "Run `touch kiln-live.txt` with the Bash tool, then reply with the word done.";
        commands
            .send_blocking(SessionCommand::Send(prompt.into()))
            .unwrap();
        let mut seen = Vec::new();
        while let Ok(event) = events.recv_blocking() {
            let answer = match &event {
                AgentEvent::PermissionRequest(request) => Some(SessionCommand::Permission {
                    request_id: request.id.clone(),
                    decision: Decision::Allow,
                }),
                _ => None,
            };
            let turn_over = matches!(event, AgentEvent::TurnEnded(_));
            seen.push(event);
            if let Some(answer) = answer {
                commands.send_blocking(answer).unwrap();
            }
            if turn_over {
                commands.close();
            }
        }
        driver.join().unwrap();
        assert!(
            seen.iter()
                .any(|e| matches!(e, AgentEvent::PermissionRequest(_))),
            "{seen:#?}"
        );
        assert!(
            seen.contains(&AgentEvent::TurnEnded(TurnOutcome::Completed)),
            "{seen:#?}"
        );
        assert!(dir.join("kiln-live.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
