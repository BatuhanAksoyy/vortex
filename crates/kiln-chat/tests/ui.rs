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
        })
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
            assert!(window.find_by_label("Allow").visible());
            window.click(("Allow", 3_u64), cx);
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
                window.try_find_by_label("Allow").is_none(),
                "buttons stay after deciding"
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
