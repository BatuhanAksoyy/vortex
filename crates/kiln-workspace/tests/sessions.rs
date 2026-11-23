//! The sidebar's session list: stored conversations open in the chat, and
//! "New session" starts over.

// Inside a `cfg(test)` module so helpers may `expect`, like unit tests.
#[cfg(test)]
mod sessions {
    use std::{
        cell::RefCell,
        path::{Path, PathBuf},
        rc::Rc,
        time::{Duration, SystemTime},
    };

    use gpui_kit::test::{TestQueryExt, TestWindowExt};
    use gpui_kit::{
        AnyWindowHandle, AppContext, Bounds, TestAppContext, WindowBounds, WindowOptions, px, size,
    };
    use kiln_agent::{
        AgentBackend, PastMessage, PastSession, Pending, Session, SessionSpec, SignIn,
    };
    use kiln_workspace::Workspace;

    /// Hands out two stored conversations and records each process start.
    #[derive(Clone, Default)]
    struct Stored(Rc<RefCell<Vec<SessionSpec>>>);

    impl AgentBackend for Stored {
        fn spawn(&self, spec: SessionSpec) -> anyhow::Result<Session> {
            self.0.borrow_mut().push(spec);
            let (events_tx, events) = async_channel::unbounded();
            let (commands, commands_rx) = async_channel::unbounded();
            Ok(Session {
                events,
                commands,
                driver: Box::pin(hold(events_tx, commands_rx)),
            })
        }

        fn sign_in(&self, _: Vec<(String, String)>) -> anyhow::Result<SignIn> {
            anyhow::bail!("no sign-in in workspace tests")
        }

        fn past_sessions(&self, _: &Path) -> Pending<Vec<PastSession>> {
            let now = SystemTime::now();
            Box::pin(async move {
                Ok(vec![
                    session("s-new", "Fix the build", now - Duration::from_mins(2)),
                    session("s-old", "Plan the release", now - Duration::from_hours(72)),
                    PastSession {
                        open_elsewhere: true,
                        ..session("s-busy", "Review the PR", now - Duration::from_hours(80))
                    },
                ])
            })
        }

        fn past_messages(&self, _: &Path, id: &str) -> Pending<Vec<PastMessage>> {
            let title = match id {
                "s-new" => "Fix the build",
                "s-busy" => "Review the PR",
                _ => "Plan the release",
            };
            Box::pin(async move { Ok(vec![PastMessage::User(title.into())]) })
        }
    }

    /// Holds both test ends open for the session's length.
    async fn hold(
        _events: async_channel::Sender<kiln_agent::AgentEvent>,
        commands: async_channel::Receiver<kiln_agent::SessionCommand>,
    ) {
        while commands.recv().await.is_ok() {}
    }

    fn no_accounts(cx: &mut gpui_kit::App) -> gpui_kit::Entity<kiln_accounts::Accounts> {
        let dir = std::env::temp_dir().join("kiln-session-tests");
        cx.new(|_| kiln_accounts::Accounts::empty(dir))
    }

    fn session(id: &str, title: &str, updated_at: SystemTime) -> PastSession {
        PastSession {
            id: id.into(),
            title: title.into(),
            updated_at,
            open_elsewhere: false,
        }
    }

    fn open(cx: &mut TestAppContext, backend: Stored) -> AnyWindowHandle {
        cx.update(gpui_kit::init);
        cx.update(kiln_workspace::init);
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let (window, _) = cx.update(|cx| {
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    gpui_kit::Point::default(),
                    size(px(1280.), px(800.)),
                ))),
                ..Default::default()
            };
            gpui_kit::open_window(options, cx, |window, cx| {
                let accounts = no_accounts(cx);
                cx.new(|cx| Workspace::new(root, Rc::new(backend), accounts, window, cx))
            })
            .expect("open test window")
        });
        cx.run_until_parked();
        window
    }

    fn click(cx: &mut TestAppContext, window: AnyWindowHandle, id: &'static str) {
        cx.update_window(window, |_, window, cx| window.click(id, cx))
            .expect("window");
        cx.run_until_parked();
    }

    fn labels(cx: &mut TestAppContext, window: AnyWindowHandle, label: &'static str) -> usize {
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            window.find_all_by_label(label).len()
        })
        .expect("window")
    }

    fn last_resume(backend: &Stored) -> Option<String> {
        backend
            .0
            .borrow()
            .last()
            .and_then(|spec| spec.resume.clone())
    }

    fn last_forks(backend: &Stored) -> bool {
        backend.0.borrow().last().is_some_and(|spec| spec.fork)
    }

    #[gpui_kit::test]
    fn the_sidebar_lists_the_stored_sessions(cx: &mut TestAppContext) {
        let window = open(cx, Stored::default());
        assert_eq!(labels(cx, window, "Fix the build"), 1);
        assert_eq!(labels(cx, window, "Plan the release"), 1);
        assert_eq!(labels(cx, window, "New session"), 1);
        let top = |cx: &mut TestAppContext, id: &'static str| {
            cx.update_window(window, |_, window, _| window.find(id).bounds().origin.y)
                .expect("window")
        };
        assert!(
            top(cx, "session-s-new") < top(cx, "session-s-old"),
            "newest first"
        );
    }

    #[gpui_kit::test]
    fn a_stored_session_opens_in_the_chat_and_resumes(cx: &mut TestAppContext) {
        let backend = Stored::default();
        let window = open(cx, backend.clone());
        assert_eq!(last_resume(&backend), None);
        click(cx, window, "session-s-old");
        assert_eq!(last_resume(&backend).as_deref(), Some("s-old"));
        assert_eq!(
            labels(cx, window, "Plan the release"),
            2,
            "listed and shown"
        );
    }

    #[gpui_kit::test]
    fn new_session_starts_over(cx: &mut TestAppContext) {
        let backend = Stored::default();
        let window = open(cx, backend.clone());
        click(cx, window, "session-s-new");
        let started = backend.0.borrow().len();
        click(cx, window, "session-new");
        assert_eq!(backend.0.borrow().len(), started + 1);
        assert_eq!(last_resume(&backend), None);
        assert_eq!(labels(cx, window, "Fix the build"), 1, "only listed");
    }

    #[gpui_kit::test]
    fn a_session_open_in_another_window_continues_as_a_copy(cx: &mut TestAppContext) {
        let backend = Stored::default();
        let window = open(cx, backend.clone());
        assert_eq!(labels(cx, window, "Open in another window"), 1);
        click(cx, window, "session-s-old");
        assert!(!last_forks(&backend), "not open elsewhere: resumed itself");
        click(cx, window, "session-s-busy");
        assert_eq!(last_resume(&backend).as_deref(), Some("s-busy"));
        assert!(last_forks(&backend));
    }
}
