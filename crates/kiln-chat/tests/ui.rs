//! Chat panel UI tests against a scripted backend: real clicks and keys, no
//! agent process.

// Inside a `cfg(test)` module so helpers may `expect`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod chat {
    use gpui_kit::TestAppContext;
    use gpui_kit::test::{TestQueryExt, TestWindowExt};
    use kiln_agent::{
        AgentEvent, Decision, PermissionRequest, Remember, RuleScope, SessionCommand, ToolCall,
        TurnOutcome,
    };
    use kiln_chat::Status;
    use serde_json::json;

    use super::common::Harness;

    fn permission() -> AgentEvent {
        AgentEvent::PermissionRequest(PermissionRequest {
            id: "req-1".into(),
            tool: "Bash".into(),
            input: json!({ "command": "touch a.txt" }),
            description: None,
            remember: None,
            questions: Vec::new(),
        })
    }

    fn deny(feedback: Option<&str>) -> SessionCommand {
        SessionCommand::Permission {
            request_id: "req-1".into(),
            decision: Decision::Deny {
                feedback: feedback.map(str::to_owned),
            },
        }
    }

    #[gpui_kit::test]
    fn enter_sends_and_allow_answers_the_permission(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        assert_eq!(
            harness.script.borrow().spawned,
            1,
            "the process starts with the panel"
        );

        harness.type_and_send(cx, "make a file");
        assert_eq!(
            harness.commands(),
            [SessionCommand::Send("make a file".into())]
        );
        assert_eq!(harness.status(cx), Status::Running);

        harness.emit(
            cx,
            AgentEvent::ToolCall(ToolCall {
                id: "tool-1".into(),
                parent: None,
                name: "Bash".into(),
                input: json!({ "command": "touch a.txt" }),
            }),
        );
        harness.emit(cx, permission());
        harness.with_window(cx, |window, cx| {
            assert!(window.find_by_label("Permission prompt").visible());
            window.click("permission-yes", cx);
        });
        assert_eq!(
            harness.commands(),
            [SessionCommand::Permission {
                request_id: "req-1".into(),
                decision: Decision::Allow
            }]
        );
        harness.with_window(cx, |window, _| {
            assert!(
                window.try_find_by_label("Permission prompt").is_none(),
                "the prompt closes once answered"
            );
        });

        harness.emit(
            cx,
            AgentEvent::ToolResult {
                id: "tool-1".into(),
                output: String::new(),
                is_error: false,
            },
        );
        harness.emit(cx, AgentEvent::TurnEnded(TurnOutcome::Completed));
        assert_eq!(harness.status(cx), Status::Idle);
    }

    #[gpui_kit::test]
    fn number_keys_answer_the_permission_prompt(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.type_and_send(cx, "make a file");
        harness.commands();
        harness.emit(cx, permission());
        harness.with_window(cx, |window, cx| window.press("2", cx));
        assert_eq!(
            harness.commands(),
            [SessionCommand::Permission {
                request_id: "req-1".into(),
                decision: Decision::Deny { feedback: None }
            }]
        );
    }

    #[gpui_kit::test]
    fn a_permission_prompt_never_takes_keys_typed_elsewhere(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.type_and_send(cx, "make a file");
        harness.commands();
        harness.with_window(cx, gpui_kit::Window::blur);
        harness.emit(cx, permission());
        harness.with_window(cx, |window, cx| window.press("1", cx));
        assert_eq!(harness.commands(), []);
        harness.with_window(cx, |window, cx| window.click("permission-title", cx));
        harness.with_window(cx, |window, cx| window.press("1", cx));
        assert_eq!(
            harness.commands(),
            [SessionCommand::Permission {
                request_id: "req-1".into(),
                decision: Decision::Allow
            }]
        );
    }

    #[gpui_kit::test]
    fn an_interrupted_turn_drops_its_permission_prompt(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.type_and_send(cx, "make a file");
        harness.emit(cx, permission());
        harness.emit(cx, AgentEvent::TurnEnded(TurnOutcome::Interrupted));
        harness.with_window(cx, |window, _| {
            assert!(window.try_find_by_label("Permission prompt").is_none());
        });
    }

    #[gpui_kit::test]
    fn feedback_typed_in_the_prompt_reaches_the_agent(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.type_and_send(cx, "make a file");
        harness.commands();
        harness.emit(cx, permission());
        harness.with_window(cx, |window, cx| window.click("permission-feedback", cx));
        harness.with_window(cx, |window, cx| window.input("use /tmp instead", cx));
        harness.with_window(cx, |window, cx| window.press("enter", cx));
        assert_eq!(harness.commands(), [deny(Some("use /tmp instead"))]);
    }

    #[gpui_kit::test]
    fn arrows_and_enter_pick_an_option(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.type_and_send(cx, "make a file");
        harness.commands();
        harness.emit(cx, permission());
        harness.with_window(cx, |window, cx| window.press("down", cx));
        harness.with_window(cx, |window, cx| window.press("enter", cx));
        assert_eq!(harness.commands(), [deny(None)]);
    }

    #[gpui_kit::test]
    fn dont_ask_again_is_offered_only_when_the_agent_can_remember(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.type_and_send(cx, "make a file");
        harness.commands();
        harness.emit(cx, permission());
        harness.with_window(cx, |window, _| {
            assert!(window.try_find("permission-always").is_none());
            assert!(window.try_find("permission-rule").is_none());
        });
        harness.with_window(cx, |window, cx| window.press("1", cx));
        for (n, (option, scope)) in [
            ("permission-session", RuleScope::Session),
            ("permission-always", RuleScope::Project),
            ("permission-everywhere", RuleScope::User),
        ]
        .into_iter()
        .enumerate()
        {
            let mut remembered = permission();
            let id = format!("req-{n}");
            if let AgentEvent::PermissionRequest(request) = &mut remembered {
                request.id.clone_from(&id);
                request.remember = Some(Remember::Rules(vec!["Bash(touch:*)".into()]));
            }
            harness.commands();
            harness.emit(cx, remembered);
            let rule =
                harness.with_window(cx, |window, _| window.try_find("permission-rule").is_some());
            assert!(rule, "the card names the rule it would keep");
            harness.with_window(cx, |window, cx| window.click(option, cx));
            assert_eq!(
                harness.commands(),
                [SessionCommand::Permission {
                    request_id: id,
                    decision: Decision::AllowAlways(scope),
                }]
            );
        }
    }

    #[gpui_kit::test]
    fn new_conversation_starts_a_fresh_process(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.type_and_send(cx, "one");
        harness.emit(cx, started("s-1"));
        harness.with_window(cx, |window, cx| window.click("chat-new", cx));
        assert_eq!(harness.status(cx), Status::Idle);
        harness.type_and_send(cx, "two");
        assert_eq!(harness.script.borrow().spawned, 2);
        assert_eq!(resumed(&harness), None, "a new conversation");
        assert_eq!(harness.commands(), [SessionCommand::Send("two".into())]);
    }

    #[gpui_kit::test]
    fn stop_interrupts_a_running_turn(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.type_and_send(cx, "count");
        harness.commands();
        harness.with_window(cx, |window, cx| window.click("chat-stop", cx));
        assert_eq!(harness.commands(), [SessionCommand::Interrupt]);
    }

    #[gpui_kit::test]
    fn a_new_process_resumes_after_the_previous_one_exits(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.type_and_send(cx, "one");
        harness.emit(cx, started("s-1"));
        harness.emit(
            cx,
            AgentEvent::Exited {
                error: Some("crashed".into()),
            },
        );
        assert_eq!(harness.status(cx), Status::Ended);
        harness.type_and_send(cx, "two");
        assert_eq!(harness.script.borrow().spawned, 2);
        assert_eq!(resumed(&harness).as_deref(), Some("s-1"));
        assert_eq!(harness.commands(), [SessionCommand::Send("two".into())]);
    }

    fn started(session_id: &str) -> AgentEvent {
        AgentEvent::Started {
            session_id: session_id.into(),
            model: "claude-opus-5-5".into(),
        }
    }

    /// The session the last process was started to continue.
    fn resumed(harness: &Harness) -> Option<String> {
        let script = harness.script.borrow();
        script
            .last_spec
            .as_ref()
            .and_then(|spec| spec.resume.clone())
    }
}
