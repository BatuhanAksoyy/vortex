//! UI integration tests: real keystrokes through the production keymap.

// Inside a `cfg(test)` module so helpers may `expect`, like unit tests.
#[cfg(test)]
mod workspace {
    use std::path::PathBuf;
    use std::time::Duration;

    use gpui_kit::test::{TestAppContextExt, TestQueryExt, TestWindowExt};
    use gpui_kit::{
        AppContext, Bounds, Entity, TestAppContext, WindowBounds, WindowHandle, WindowOptions, px,
        size,
    };
    use kiln_workspace::Workspace;

    /// The chat starts its agent when it opens; these tests need none.
    struct NoAgent;

    impl kiln_agent::AgentBackend for NoAgent {
        fn spawn(&self, _: kiln_agent::SessionSpec) -> anyhow::Result<kiln_agent::Session> {
            anyhow::bail!("no agent in workspace tests")
        }

        fn sign_in(&self, _: Vec<(String, String)>) -> anyhow::Result<kiln_agent::SignIn> {
            anyhow::bail!("no agent in workspace tests")
        }

        fn past_sessions(
            &self,
            _: &std::path::Path,
        ) -> kiln_agent::Pending<Vec<kiln_agent::PastSession>> {
            Box::pin(async { Ok(Vec::new()) })
        }

        fn past_messages(
            &self,
            _: &std::path::Path,
            _: &str,
        ) -> kiln_agent::Pending<Vec<kiln_agent::PastMessage>> {
            Box::pin(async { anyhow::bail!("no stored conversations in workspace tests") })
        }
    }

    /// Workspace tests add no account, so the list is never written.
    fn no_accounts(cx: &mut gpui_kit::App) -> Entity<kiln_accounts::Accounts> {
        let dir = std::env::temp_dir().join("kiln-workspace-tests");
        cx.new(|_| kiln_accounts::Accounts::empty(dir))
    }

    fn open_workspace(
        cx: &mut TestAppContext,
        root: PathBuf,
    ) -> (WindowHandle<gpui_kit::base::Root>, Entity<Workspace>) {
        cx.update(gpui_kit::init);
        cx.update(kiln_workspace::init);
        let (window, workspace) = cx.update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        gpui_kit::Point::default(),
                        size(px(1280.), px(800.)),
                    ))),
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    let backend = std::rc::Rc::new(NoAgent);
                    let accounts = no_accounts(cx);
                    cx.new(|cx| Workspace::new(root, backend, accounts, window, cx))
                },
            )
            .expect("open test window")
        });
        cx.run_until_parked();
        (window.downcast().expect("kit window has a Root"), workspace)
    }

    /// The production keymap binds `cmd` on macOS where it binds `ctrl`
    /// elsewhere.
    fn primary(key: &str) -> String {
        let modifier = if cfg!(target_os = "macos") {
            "cmd"
        } else {
            "ctrl"
        };
        format!("{modifier}-{key}")
    }

    fn project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::create_dir_all(dir.path().join("src")).expect("mkdir");
        std::fs::write(dir.path().join("src/main.rs"), "fn main() {}\n").expect("write");
        std::fs::write(dir.path().join("README.md"), "# demo\n").expect("write");
        dir
    }

    #[gpui_kit::test]
    async fn ctrl_p_opens_quick_open_and_enter_opens_the_file(cx: &mut TestAppContext) {
        let dir = project();
        let root = std::fs::canonicalize(dir.path()).expect("canonical root");
        let (window, workspace) = open_workspace(cx, root.clone());
        let any_window = window.into();

        cx.update_window(any_window, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("quick-open").is_none());
            window.press(&primary("p"), cx);
        })
        .expect("window");
        cx.wait_for(any_window, Duration::from_secs(2), |window, _| {
            window.try_find("quick-open").is_some()
        })
        .await;

        cx.update_window(any_window, |_, window, cx| window.input("main", cx))
            .expect("window");
        cx.run_until_parked();
        cx.update_window(any_window, |_, window, cx| window.press("enter", cx))
            .expect("window");
        cx.run_until_parked();

        let active = cx.update(|cx| workspace.read(cx).active_file(cx));
        assert_eq!(active, Some(root.join("src/main.rs")));
        let editor_height = cx
            .update_window(any_window, |_, window, cx| {
                window.render_frame(cx);
                window.find_by_label("Editor main.rs").bounds().size.height
            })
            .expect("window");
        assert!(
            editor_height > px(600.),
            "editor does not fill its pane: {editor_height:?}"
        );
    }

    fn press(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle, key: &str) {
        cx.update_window(window, |_, window, cx| window.press(key, cx))
            .expect("window");
        cx.run_until_parked();
    }

    fn type_text(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle, text: &str) {
        cx.update_window(window, |_, window, cx| window.input(text, cx))
            .expect("window");
        cx.run_until_parked();
    }

    fn has(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle, id: &'static str) -> bool {
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            window.try_find(id).is_some()
        })
        .expect("window")
    }

    #[gpui_kit::test]
    fn command_palette_runs_the_selected_command(cx: &mut TestAppContext) {
        let dir = project();
        let (window, _) = open_workspace(cx, dir.path().to_path_buf());
        let window = window.into();
        assert!(has(cx, window, "session-list"));

        press(cx, window, &primary("shift-p"));
        type_text(cx, window, "Toggle Sidebar");
        press(cx, window, "enter");

        assert!(!has(cx, window, "session-list"));
    }

    #[gpui_kit::test]
    fn clicking_the_close_button_of_the_last_tab_closes_it(cx: &mut TestAppContext) {
        let dir = project();
        let root = std::fs::canonicalize(dir.path()).expect("canonical root");
        let (window, workspace) = open_workspace(cx, root.clone());
        let window = window.into();
        press(cx, window, &primary("p"));
        type_text(cx, window, "README");
        press(cx, window, "enter");
        assert_eq!(
            cx.update(|cx| workspace.read(cx).active_file(cx)),
            Some(root.join("README.md"))
        );

        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            window.click(("close-editor-tab", 0_u64), cx);
        })
        .expect("window");
        cx.run_until_parked();
        assert_eq!(cx.update(|cx| workspace.read(cx).active_file(cx)), None);
        assert!(
            !has(cx, window, "editor-pane"),
            "an empty pane only takes room from the chat"
        );
    }

    #[gpui_kit::test]
    fn close_editor_keeps_a_tab_with_unsaved_changes(cx: &mut TestAppContext) {
        let dir = project();
        let root = std::fs::canonicalize(dir.path()).expect("canonical root");
        let (window, workspace) = open_workspace(cx, root.clone());
        let window = window.into();
        let active = |cx: &mut TestAppContext| cx.update(|cx| workspace.read(cx).active_file(cx));

        press(cx, window, &primary("p"));
        type_text(cx, window, "README");
        press(cx, window, "enter");
        assert_eq!(active(cx), Some(root.join("README.md")));

        type_text(cx, window, "edit");
        press(cx, window, &primary("w"));
        assert_eq!(
            active(cx),
            Some(root.join("README.md")),
            "dirty tab closed without asking"
        );

        press(cx, window, "escape");
        press(cx, window, &primary("s"));
        press(cx, window, &primary("w"));
        assert_eq!(active(cx), None);
        let saved = std::fs::read_to_string(root.join("README.md")).expect("read");
        assert!(saved.contains("edit"), "{saved}");
    }
}
