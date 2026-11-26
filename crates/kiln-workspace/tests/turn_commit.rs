//! Committing a chat's work for it: with `git.commit.trigger` at
//! `task_done`, once a turn ends with the work done as the writer judges
//! it; at any time with the chat's Commit button. Only the files the chat's
//! turns changed go in, with a message the writer wrote by the project's
//! directives, and the commit waits for the user. A chat's own settings go
//! over the project's.

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

    /// Ends a turn when the test says so, and answers the writer.
    #[derive(Clone)]
    struct Agent {
        end: Receiver<()>,
        asked: Rc<RefCell<Vec<OneShot>>>,
        answer: &'static str,
    }

    impl AgentBackend for Agent {
        fn name(&self) -> &'static str {
            "claude"
        }

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
            let answer = self.answer.to_owned();
            Box::pin(async move { Ok(answer) })
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

    /// A committed project with `commit` as its commit settings, and the
    /// user's own edit left in it.
    fn project(commit: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        git(root, &["init", "-q", "-b", "main"]);
        git(root, &["config", "user.name", "Kiln"]);
        git(root, &["config", "user.email", "kiln@example.com"]);
        std::fs::create_dir(root.join(".kiln")).expect("mkdir");
        let settings =
            format!(r#"{{"git": {{"commit": {commit}, "push": {{"policy": "never"}}}}}}"#);
        std::fs::write(root.join(".kiln/settings.json"), settings).expect("write");
        std::fs::write(root.join("mine.txt"), "mine\n").expect("write");
        git(root, &["add", "."]);
        git(root, &["commit", "-q", "-m", "start"]);
        std::fs::write(root.join("mine.txt"), "mine, edited\n").expect("write");
        dir
    }

    /// The test's ends of a chat: its window, the turn it ends and what
    /// the writer was asked.
    struct Chat {
        window: AnyWindowHandle,
        end: Sender<()>,
        asked: Rc<RefCell<Vec<OneShot>>>,
    }

    fn open(cx: &mut TestAppContext, root: &Path, answer: &'static str) -> Chat {
        let (end, end_rx) = async_channel::unbounded();
        let asked = Rc::default();
        let agent = Agent {
            end: end_rx,
            asked: Rc::clone(&asked),
            answer,
        };
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
        Chat { window, end, asked }
    }

    fn act(
        cx: &mut TestAppContext,
        window: AnyWindowHandle,
        act: impl FnOnce(&mut gpui_kit::Window, &mut gpui_kit::App),
    ) {
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            act(window, cx);
        })
        .expect("window");
        cx.run_until_parked();
    }

    async fn shown(cx: &mut TestAppContext, chat: &Chat, id: &'static str) {
        cx.wait_for(chat.window, WAIT, |window, _| window.try_find(id).is_some())
            .await;
    }

    /// Sends a message, writes a file as the agent would, and ends the
    /// turn; the chat then shows the file waiting for a commit.
    async fn turn(cx: &mut TestAppContext, chat: &Chat, root: &Path) {
        act(cx, chat.window, |window, cx| {
            window.click("chat-composer", cx);
        });
        act(cx, chat.window, |window, cx| {
            window.input("write some notes", cx);
        });
        act(cx, chat.window, |window, cx| window.press("enter", cx));
        std::fs::write(root.join("notes.txt"), "notes\n").expect("write");
        chat.end.send_blocking(()).expect("end the turn");
        shown(cx, chat, "chat-work").await;
    }

    async fn commit(cx: &mut TestAppContext, chat: &Chat, root: &Path) {
        act(cx, chat.window, |window, cx| {
            window.click("commit-review-commit", cx);
        });
        cx.wait_for(chat.window, WAIT, |window, _| {
            window.try_find("commit-review").is_none()
        })
        .await;
        let message = git(root, &["log", "-1", "--format=%B"]);
        assert_eq!(message, "Add the agent's notes", "the credit is taken out");
        assert_eq!(git(root, &["status", "--porcelain"]), "M mine.txt");
    }

    #[gpui_kit::test]
    async fn a_finished_turn_gets_a_commit_the_writer_wrote(cx: &mut TestAppContext) {
        let dir = project(
            r#"{"enabled": true, "trigger": "task_done", "instructions": ["Write in English."]}"#,
        );
        let root = dir.path();
        let chat = open(cx, root, WRITTEN);
        turn(cx, &chat, root).await;
        shown(cx, &chat, "commit-review").await;
        {
            let asked = chat.asked.borrow();
            assert_eq!(asked.len(), 1);
            assert_eq!(asked[0].model, "haiku");
            assert!(
                asked[0].system.contains("NOT_DONE"),
                "it judges the work first"
            );
            assert!(asked[0].system.ends_with("- Write in English."));
            assert!(asked[0].prompt.contains("write some notes"));
            assert!(asked[0].prompt.contains("notes.txt"));
            assert!(
                !asked[0].prompt.contains("mine.txt"),
                "the user's edit is not the chat's"
            );
        }
        commit(cx, &chat, root).await;
    }

    #[gpui_kit::test]
    async fn an_unfinished_turn_is_left_to_the_user(cx: &mut TestAppContext) {
        let dir = project(r#"{"enabled": true, "trigger": "task_done"}"#);
        let root = dir.path();
        let chat = open(cx, root, "NOT_DONE");
        turn(cx, &chat, root).await;
        let asked = Rc::clone(&chat.asked);
        cx.wait_for(chat.window, WAIT, move |_, _| !asked.borrow().is_empty())
            .await;
        let mut reviewing = true;
        act(cx, chat.window, |window, _| {
            reviewing = window.try_find("commit-review").is_some();
        });
        assert!(!reviewing, "nothing is offered");
        assert_eq!(git(root, &["log", "--format=%s"]), "start");
    }

    #[gpui_kit::test]
    async fn the_button_commits_what_the_chat_changed(cx: &mut TestAppContext) {
        let dir = project(r#"{"enabled": true, "trigger": "agent"}"#);
        let root = dir.path();
        let chat = open(cx, root, WRITTEN);
        turn(cx, &chat, root).await;
        assert!(chat.asked.borrow().is_empty(), "nothing is written unasked");
        act(cx, chat.window, |window, cx| {
            window.click("chat-work-commit", cx);
        });
        shown(cx, &chat, "commit-review").await;
        let judged = chat.asked.borrow()[0].system.contains("NOT_DONE");
        assert!(!judged, "asked for, not judged");
        commit(cx, &chat, root).await;
    }

    #[gpui_kit::test]
    async fn a_chats_own_settings_go_over_the_projects(cx: &mut TestAppContext) {
        let dir = project(r#"{"enabled": true, "trigger": "agent"}"#);
        let root = dir.path();
        let file = root.join(".kiln/settings.json");
        let shared = std::fs::read_to_string(&file).expect("read");
        let chat = open(cx, root, WRITTEN);
        act(cx, chat.window, |window, cx| {
            window.click("chat-commit-settings", cx);
        });
        shown(cx, &chat, "commit-settings").await;
        act(cx, chat.window, |window, cx| {
            window.click("commit-settings-trigger", cx);
        });
        // The open picker's list has the focus; its second choice is task_done.
        act(cx, chat.window, |window, cx| window.press("down", cx));
        act(cx, chat.window, |window, cx| window.press("enter", cx));
        act(cx, chat.window, |window, cx| window.press("escape", cx));
        let kept = std::fs::read_to_string(&file).expect("read");
        assert_eq!(kept, shared, "the project's settings stay as they were");

        turn(cx, &chat, root).await;
        shown(cx, &chat, "commit-review").await;
        commit(cx, &chat, root).await;
    }

    #[gpui_kit::test]
    async fn the_git_page_opens_the_projects_tab(cx: &mut TestAppContext) {
        let dir = project(r#"{"trigger": "agent"}"#);
        let root = dir.path();
        let shared = std::fs::read_to_string(root.join(".kiln/settings.json")).expect("read");
        let chat = open(cx, root, WRITTEN);
        act(cx, chat.window, |window, cx| {
            window.click("activity-git", cx);
        });
        shown(cx, &chat, "git-commit-settings").await;
        act(cx, chat.window, |window, cx| {
            window.click("git-commit-settings", cx);
        });
        shown(cx, &chat, "commit-settings").await;
        let kept = std::fs::read_to_string(root.join(".kiln/settings.json")).expect("read");
        assert_eq!(kept, shared, "opening writes nothing");
    }

    #[gpui_kit::test]
    fn by_default_kiln_leaves_committing_to_the_user(cx: &mut TestAppContext) {
        let dir = project(r#"{"trigger": "task_done"}"#);
        let root = dir.path();
        let chat = open(cx, root, WRITTEN);
        act(cx, chat.window, |window, cx| {
            window.click("chat-composer", cx);
        });
        act(cx, chat.window, |window, cx| {
            window.input("write some notes", cx);
        });
        act(cx, chat.window, |window, cx| window.press("enter", cx));
        std::fs::write(root.join("notes.txt"), "notes\n").expect("write");
        chat.end.send_blocking(()).expect("end the turn");
        cx.executor().advance_clock(Duration::from_secs(1));
        let mut strip = true;
        act(cx, chat.window, |window, _| {
            strip = window.try_find("chat-work").is_some();
        });
        assert!(!strip, "no strip");
        assert!(chat.asked.borrow().is_empty(), "no writer");
        assert_eq!(git(root, &["log", "--format=%s"]), "start");
    }

    #[gpui_kit::test]
    async fn work_the_user_undid_is_no_longer_counted(cx: &mut TestAppContext) {
        let dir = project(r#"{"enabled": true, "trigger": "agent"}"#);
        let root = dir.path();
        let chat = open(cx, root, WRITTEN);
        turn(cx, &chat, root).await;
        std::fs::remove_file(root.join("notes.txt")).expect("undo");
        act(cx, chat.window, |window, cx| {
            window.click("activity-git", cx);
        });
        shown(cx, &chat, "git-refresh").await;
        act(cx, chat.window, |window, cx| {
            window.click("git-refresh", cx);
        });
        cx.wait_for(chat.window, WAIT, |window, _| {
            window.try_find("chat-work").is_none()
        })
        .await;
    }
}
