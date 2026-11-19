//! Chat panel UI tests against a scripted backend: real clicks and keys, no
//! agent process.

// Inside a `cfg(test)` module so helpers may `expect`, like unit tests.
#[cfg(test)]
mod chat {
    use std::{cell::RefCell, rc::Rc};

    use gpui_kit::test::{TestQueryExt, TestWindowExt};
    use gpui_kit::{
        AnyWindowHandle, AppContext, Bounds, Entity, TestAppContext, WindowBounds, WindowOptions,
        px, size,
    };
    use kiln_agent::{
        AgentBackend, AgentEvent, Decision, PermissionRequest, Session, SessionCommand,
        SessionSpec, ToolCall, TurnOutcome,
    };
    use kiln_chat::{ChatView, Status};
    use serde_json::json;

    /// The test's ends of the most recently spawned session.
    #[derive(Default)]
    struct Script {
        events: Option<async_channel::Sender<AgentEvent>>,
        commands: Option<async_channel::Receiver<SessionCommand>>,
        spawned: usize,
    }

    struct FakeBackend(Rc<RefCell<Script>>);

    impl AgentBackend for FakeBackend {
        fn spawn(&self, _: SessionSpec) -> anyhow::Result<Session> {
            let (event_tx, events) = async_channel::unbounded();
            let (commands, command_rx) = async_channel::unbounded();
            let mut script = self.0.borrow_mut();
            script.events = Some(event_tx);
            script.commands = Some(command_rx);
            script.spawned += 1;
            Ok(Session {
                events,
                commands,
                driver: Box::pin(async {}),
            })
        }
    }

    struct Harness {
        window: AnyWindowHandle,
        view: Entity<ChatView>,
        script: Rc<RefCell<Script>>,
    }

    impl Harness {
        fn new(cx: &mut TestAppContext) -> Self {
            cx.update(gpui_kit::init);
            let script = Rc::new(RefCell::new(Script::default()));
            let backend = Rc::new(FakeBackend(script.clone()));
            let (window, view) = cx.update(|cx| open(backend, cx));
            cx.run_until_parked();
            Self {
                window,
                view,
                script,
            }
        }

        fn emit(&self, cx: &mut TestAppContext, event: AgentEvent) {
            let sender = self
                .script
                .borrow()
                .events
                .clone()
                .expect("session started");
            sender.try_send(event).expect("send event");
            cx.run_until_parked();
        }

        fn commands(&self) -> Vec<SessionCommand> {
            let script = self.script.borrow();
            let receiver = script.commands.as_ref().expect("session started");
            std::iter::from_fn(|| receiver.try_recv().ok()).collect()
        }

        fn with_window<R>(
            &self,
            cx: &mut TestAppContext,
            f: impl FnOnce(&mut gpui_kit::Window, &mut gpui_kit::App) -> R,
        ) -> R {
            let result = cx
                .update_window(self.window, |_, window, cx| {
                    window.render_frame(cx);
                    f(window, cx)
                })
                .expect("window");
            cx.run_until_parked();
            result
        }

        fn type_and_send(&self, cx: &mut TestAppContext, text: &str) {
            self.with_window(cx, |window, cx| window.click("chat-composer", cx));
            self.with_window(cx, |window, cx| window.input(text, cx));
            self.with_window(cx, |window, cx| window.press("enter", cx));
        }

        fn status(&self, cx: &mut TestAppContext) -> Status {
            cx.update(|cx| self.view.read(cx).status())
        }
    }

    fn open(
        backend: Rc<FakeBackend>,
        cx: &mut gpui_kit::App,
    ) -> (AnyWindowHandle, Entity<ChatView>) {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                gpui_kit::Point::default(),
                size(px(800.), px(700.)),
            ))),
            ..Default::default()
        };
        let build = |window: &mut gpui_kit::Window, cx: &mut gpui_kit::App| {
            cx.new(|cx| ChatView::new(backend, std::env::temp_dir(), window, cx))
        };
        gpui_kit::open_window(options, cx, build).expect("open test window")
    }

    fn permission() -> AgentEvent {
        AgentEvent::PermissionRequest(PermissionRequest {
            id: "req-1".into(),
            tool: "Bash".into(),
            input: json!({ "command": "touch a.txt" }),
            description: None,
            remember: None,
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
            0,
            "no process before the first message"
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
            assert!(
                window
                    .try_find_by_label("Yes, and don't ask again")
                    .is_none()
            );
        });
        harness.with_window(cx, |window, cx| window.press("1", cx));
        let mut remembered = permission();
        if let AgentEvent::PermissionRequest(request) = &mut remembered {
            request.id = "req-2".into();
            request.remember = Some(kiln_agent::Remember::Rule);
        }
        harness.commands();
        harness.emit(cx, remembered);
        harness.with_window(cx, |window, cx| window.click("permission-always", cx));
        assert_eq!(
            harness.commands(),
            [SessionCommand::Permission {
                request_id: "req-2".into(),
                decision: Decision::AllowAlways
            }]
        );
    }

    #[gpui_kit::test]
    fn new_conversation_starts_a_fresh_process(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.type_and_send(cx, "one");
        harness.with_window(cx, |window, cx| window.click("chat-new", cx));
        assert_eq!(harness.status(cx), Status::Idle);
        harness.type_and_send(cx, "two");
        assert_eq!(harness.script.borrow().spawned, 2);
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
    fn a_new_process_starts_after_the_previous_one_exits(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.type_and_send(cx, "one");
        harness.emit(
            cx,
            AgentEvent::Exited {
                error: Some("crashed".into()),
            },
        );
        assert_eq!(harness.status(cx), Status::Ended);
        harness.type_and_send(cx, "two");
        assert_eq!(harness.script.borrow().spawned, 2);
        assert_eq!(harness.commands(), [SessionCommand::Send("two".into())]);
    }
}
