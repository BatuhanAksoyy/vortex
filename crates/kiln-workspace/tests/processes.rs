//! The process meters: what each agent runs, and stopping it.

// Inside a `cfg(test)` module so helpers may `expect`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod processes {
    use std::{path::Path, process::Command, time::Duration};

    use gpui_kit::test::{TestQueryExt, TestWindowExt};
    use gpui_kit::{AnyWindowHandle, AppContext as _, ElementId, TestAppContext};
    use kiln_agent::{
        AgentBackend, PastMessage, PastSession, Pending, Session, SessionSpec, SignIn,
    };

    use super::common::{Stored, chats, click, labels, no_accounts, open_in, pick, spawned};

    /// A backend whose agents all run as process `pid`.
    struct Running {
        stored: Stored,
        pid: u32,
    }

    impl AgentBackend for Running {
        fn name(&self) -> &'static str {
            "claude"
        }

        fn spawn(&self, spec: SessionSpec) -> anyhow::Result<Session> {
            let session = self.stored.spawn(spec)?;
            Ok(Session {
                pid: Some(self.pid),
                ..session
            })
        }

        fn sign_in(&self, env: Vec<(String, String)>) -> anyhow::Result<SignIn> {
            self.stored.sign_in(env)
        }

        fn past_sessions(&self, cwd: &Path) -> Pending<Vec<PastSession>> {
            self.stored.past_sessions(cwd)
        }

        fn past_messages(&self, cwd: &Path, id: &str) -> Pending<Vec<PastMessage>> {
            self.stored.past_messages(cwd, id)
        }
    }

    fn has(cx: &mut TestAppContext, window: AnyWindowHandle, id: String) -> bool {
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            window.try_find(ElementId::Name(id.into())).is_some()
        })
        .expect("window")
    }

    fn type_text(cx: &mut TestAppContext, window: AnyWindowHandle, text: &str) {
        cx.update_window(window, |_, window, cx| window.input(text, cx))
            .expect("window");
        cx.run_until_parked();
    }

    fn press(cx: &mut TestAppContext, window: AnyWindowHandle, key: &str) {
        cx.update_window(window, |_, window, cx| window.press(key, cx))
            .expect("window");
        cx.run_until_parked();
    }

    /// The id of the row of the process running `command`.
    fn row_of(cx: &mut TestAppContext, window: AnyWindowHandle, command: &str) -> String {
        let id = cx
            .update_window(window, |_, window, cx| {
                window.render_frame(cx);
                let row = window.find_by_label(command);
                row.path().last().cloned().expect("an id")
            })
            .expect("window");
        let ElementId::Name(name) = id else {
            panic!("a named row");
        };
        name.to_string()
    }

    fn click_id(cx: &mut TestAppContext, window: AnyWindowHandle, id: String) {
        cx.update_window(window, |_, window, cx| {
            window.click(ElementId::Name(id.into()), cx);
        })
        .expect("window");
        cx.run_until_parked();
    }

    /// An agent that left `sleep 30` running under it.
    fn agent_with_a_child() -> std::process::Child {
        let agent = Command::new("sh")
            .args(["-c", "sleep 30 & wait $!"])
            .spawn()
            .expect("start a shell");
        // Time for the shell to start its child.
        std::thread::sleep(Duration::from_millis(300));
        agent
    }

    #[gpui_kit::test]
    fn the_chat_meter_opens_its_processes_over_the_chat(cx: &mut TestAppContext) {
        let mut agent = agent_with_a_child();
        let backend = Running {
            stored: Stored::default(),
            pid: agent.id(),
        };
        let window = open_in(
            cx,
            backend,
            None,
            no_accounts(),
            Some(env!("CARGO_MANIFEST_DIR").into()),
        );
        click(cx, window, "chat-processes");
        assert!(has(cx, window, "chat-processes-overlay".into()));
        assert_eq!(labels(cx, window, "sleep 30"), 1);
        press(cx, window, "escape");
        assert!(!has(cx, window, "chat-processes-overlay".into()));
        let _ = agent.kill();
        let _ = agent.wait();
    }

    /// A window whose agent left `sleep 30` running, with every agent's
    /// processes open over it.
    fn open_all(cx: &mut TestAppContext) -> (std::process::Child, AnyWindowHandle) {
        let agent = agent_with_a_child();
        let stored = Stored::default();
        let backend = Running {
            stored: stored.clone(),
            pid: agent.id(),
        };
        let window = open_in(
            cx,
            backend,
            None,
            no_accounts(),
            Some(env!("CARGO_MANIFEST_DIR").into()),
        );
        pick(cx, window, "s-old");
        assert!(spawned(&stored) > 0);
        assert_eq!(chats(cx, window).len(), 1, "one chat runs the agent");
        click(cx, window, "all-processes");
        (agent, window)
    }

    fn end(mut agent: std::process::Child) {
        let _ = agent.kill();
        let _ = agent.wait();
    }

    #[gpui_kit::test]
    fn the_sidebar_meter_opens_every_process_under_its_chat(cx: &mut TestAppContext) {
        let (agent, window) = open_all(cx);
        let (page, viewport) = cx
            .update_window(window, |_, window, cx| {
                window.render_frame(cx);
                let page = window.find("all-processes-overlay").bounds().size;
                (page, window.viewport_size())
            })
            .expect("window");
        assert_eq!(page.width, viewport.width, "the whole window");
        assert_eq!(
            page.height,
            viewport.height - gpui_kit::component::TITLE_BAR_HEIGHT,
            "under the title bar"
        );
        assert!(has(cx, window, format!("processes-chat-{}", agent.id())));
        click(cx, window, "processes-sort-memory");
        click(cx, window, "processes-sort-memory");
        assert!(
            !has(cx, window, format!("processes-chat-{}", agent.id())),
            "sorted rows leave their chat's heading"
        );
        press(cx, window, "escape");
        assert!(!has(cx, window, "all-processes-overlay".into()));
        end(agent);
    }

    #[gpui_kit::test]
    fn the_search_waits_for_a_click_and_matches_commands(cx: &mut TestAppContext) {
        let (agent, window) = open_all(cx);
        type_text(cx, window, "wait");
        assert_eq!(
            labels(cx, window, "sleep 30"),
            1,
            "the search has no focus yet"
        );
        click(cx, window, "processes-search");
        type_text(cx, window, "wait");
        assert_eq!(labels(cx, window, "sleep 30"), 0, "only the shell matches");
        assert!(has(cx, window, format!("process-{}", agent.id())));
        press(cx, window, "cmd-a");
        press(cx, window, "backspace");
        assert_eq!(labels(cx, window, "sleep 30"), 1);
        end(agent);
    }

    #[gpui_kit::test]
    fn details_close_by_button_second_click_or_escape(cx: &mut TestAppContext) {
        let (agent, window) = open_all(cx);
        let row = row_of(cx, window, "sleep 30");
        click_id(cx, window, row.clone());
        assert!(has(cx, window, "process-detail".into()));
        assert_eq!(labels(cx, window, "sleep"), 1, "named in the details");
        click(cx, window, "process-detail-close");
        assert!(!has(cx, window, "process-detail".into()));
        click_id(cx, window, row.clone());
        click_id(cx, window, row.clone());
        assert!(
            !has(cx, window, "process-detail".into()),
            "a second click hides them"
        );
        click_id(cx, window, row);
        press(cx, window, "escape");
        assert!(
            !has(cx, window, "process-detail".into()),
            "Escape closes the details first"
        );
        assert!(has(cx, window, "all-processes-overlay".into()));
        end(agent);
    }

    #[gpui_kit::test]
    fn a_row_stops_what_the_agent_started(cx: &mut TestAppContext) {
        let (mut agent, window) = open_all(cx);
        let stop = row_of(cx, window, "sleep 30").replace("process-", "process-stop-");
        click_id(cx, window, stop);
        let status = agent.wait().expect("the shell ends");
        assert!(!status.success(), "its child was stopped: {status}");
    }
}
