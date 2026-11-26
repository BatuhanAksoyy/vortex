//! With `git.commit.trigger` at `turn_end`, a turn that changed files
//! without committing them gets a commit with a message the small model
//! writes, which waits for the user.

#[cfg(test)]
mod turn_commit {
    use std::{cell::RefCell, path::Path, process::Command, rc::Rc, time::Duration};

    use async_channel::{Receiver, Sender};
    use gpui_kit::test::{TestAppContextExt as _, TestWindowExt as _};
    use gpui_kit::{
        AnyWindowHandle, AppContext as _, Bounds, TestAppContext, WindowBounds, WindowOptions, px,
        size,
    };
    use kiln_agent::{
        AgentBackend, AgentEvent, OneShot, PastMessage, PastSession, Pending, Session,
        SessionCommand, SessionSpec, SignIn, TurnOutcome,
    };
    use kiln_workspace::Workspace;

    const WAIT: Duration = Duration::from_secs(5);
    const WRITTEN: &str = "Add the agent's notes\n\nCo-Authored-By: Claude <noreply@anthropic.com>";

    /// Ends a turn when the test says so, and writes commit messages.
    #[derive(Clone)]
    struct Agent {
        end: Receiver<()>,
        asked: Rc<RefCell<Vec<OneShot>>>,
    }

    impl AgentBackend for Agent {
        fn spawn(&self, _: SessionSpec) -> anyhow::Result<Session> {
            let (events_tx, events) = async_channel::unbounded();
            let (commands, commands_rx) = async_channel::unbounded();
            Ok(Session {
                pid: None,
                events,
                commands,
                driver: Box::pin(work(self.end.clone(), events_tx, commands_rx)),
            })
        }

        fn sign_in(&self, _: Vec<(String, String)>) -> anyhow::Result<SignIn> {
            anyhow::bail!("no sign-in here")
        }

        fn past_sessions(&self, _: &Path) -> Pending<Vec<PastSession>> {
            Box::pin(async { Ok(Vec::new()) })
        }

        fn past_messages(&self, _: &Path, _: &str) -> Pending<Vec<PastMessage>> {
            Box::pin(async { anyhow::bail!("nothing stored") })
        }

        fn ask_once(&self, ask: OneShot) -> Pending<String> {
            self.asked.borrow_mut().push(ask);
            Box::pin(async { Ok(WRITTEN.to_owned()) })
        }
    }

    async fn work(
        end: Receiver<()>,
        events: Sender<AgentEvent>,
        commands: Receiver<SessionCommand>,
    ) {
        while let Ok(command) = commands.recv().await {
            if let SessionCommand::Send(_) = command
                && end.recv().await.is_ok()
            {
                let _ = events
                    .send(AgentEvent::TurnEnded(TurnOutcome::Completed))
                    .await;
            }
        }
    }

    fn git(root: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .expect("git");
        assert!(out.status.success(), "git {args:?}");
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    /// A committed project whose settings commit after each turn, with the
    /// user's own edit left in it.
    fn project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        git(root, &["init", "-q", "-b", "main"]);
        git(root, &["config", "user.name", "Kiln"]);
        git(root, &["config", "user.email", "kiln@example.com"]);
        std::fs::create_dir(root.join(".kiln")).expect("mkdir");
        let settings = r#"{"git": {"commit": {"trigger": "turn_end", "review": true}}}"#;
        std::fs::write(root.join(".kiln/settings.json"), settings).expect("write");
        std::fs::write(root.join("mine.txt"), "mine\n").expect("write");
        git(root, &["add", "."]);
        git(root, &["commit", "-q", "-m", "start"]);
        std::fs::write(root.join("mine.txt"), "mine, edited\n").expect("write");
        dir
    }

    fn open(cx: &mut TestAppContext, agent: Agent, root: &Path) -> AnyWindowHandle {
        cx.update(gpui_kit::init);
        cx.update(kiln_workspace::init);
        let root = root.to_path_buf();
        let (window, _) = cx.update(|cx| {
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    gpui_kit::Point::default(),
                    size(px(1280.), px(800.)),
                ))),
                ..Default::default()
            };
            gpui_kit::open_window(options, cx, |window, cx| {
                let dir = std::env::temp_dir().join("kiln-turn-commit-tests");
                let accounts = cx.new(|_| kiln_accounts::Accounts::empty(dir));
                cx.new(|cx| Workspace::new(Some(root), Rc::new(agent), accounts, window, cx))
            })
            .expect("open test window")
        });
        cx.run_until_parked();
        window
    }

    fn act(
        cx: &mut TestAppContext,
        window: AnyWindowHandle,
        act: impl FnOnce(&mut gpui_kit::Window, &mut gpui_kit::App),
    ) {
        cx.update_window(window, |_, window, cx| act(window, cx))
            .expect("window");
        cx.run_until_parked();
    }

    #[gpui_kit::test]
    async fn a_turn_that_changed_files_gets_a_written_commit(cx: &mut TestAppContext) {
        let dir = project();
        let root = dir.path();
        let (end, end_rx) = async_channel::unbounded();
        let agent = Agent {
            end: end_rx,
            asked: Rc::default(),
        };
        let window = open(cx, agent.clone(), root);
        act(cx, window, |window, cx| {
            window.click("chat-composer", cx);
        });
        act(cx, window, |window, cx| {
            window.input("write some notes", cx);
        });
        act(cx, window, |window, cx| {
            window.press("enter", cx);
        });
        std::fs::write(root.join("notes.txt"), "notes\n").expect("write");
        end.send_blocking(()).expect("end the turn");
        cx.wait_for(window, WAIT, |window, _| {
            window.try_find("commit-review").is_some()
        })
        .await;
        {
            let asked = agent.asked.borrow();
            assert_eq!(asked.len(), 1);
            assert_eq!(asked[0].model, "haiku");
            assert!(asked[0].prompt.contains("write some notes"));
            assert!(asked[0].prompt.contains("notes.txt"));
            assert!(
                !asked[0].prompt.contains("mine.txt"),
                "the user's edit is not the turn's"
            );
        }
        act(cx, window, |window, cx| {
            window.click("commit-review-commit", cx);
        });
        cx.wait_for(window, WAIT, |window, _| {
            window.try_find("commit-review").is_none()
        })
        .await;
        assert_eq!(
            git(root, &["log", "-1", "--format=%B"]),
            "Add the agent's notes"
        );
        assert_eq!(git(root, &["status", "--porcelain"]), "M mine.txt");
    }
}
