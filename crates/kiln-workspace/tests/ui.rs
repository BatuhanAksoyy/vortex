//! UI integration tests: real keystrokes through the production keymap.

// Inside a `cfg(test)` module so helpers may `expect`, like unit tests.
#[cfg(test)]
mod workspace {
    use std::path::PathBuf;
    use std::time::Duration;

    use gpui_kit::test::{TestAppContextExt, TestWindowExt};
    use gpui_kit::{
        AppContext, Bounds, Entity, TestAppContext, WindowBounds, WindowHandle, WindowOptions, px,
        size,
    };
    use kiln_workspace::Workspace;

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
                |window, cx| cx.new(|cx| Workspace::new(root, window, cx)),
            )
            .expect("open test window")
        });
        cx.run_until_parked();
        (window.downcast().expect("kit window has a Root"), workspace)
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
            window.press("ctrl-p", cx);
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
        assert!(has(cx, window, "file-tree"));

        press(cx, window, "ctrl-shift-p");
        type_text(cx, window, "Toggle File Tree");
        press(cx, window, "enter");

        assert!(!has(cx, window, "file-tree"));
    }

    #[gpui_kit::test]
    fn close_editor_keeps_a_tab_with_unsaved_changes(cx: &mut TestAppContext) {
        let dir = project();
        let root = std::fs::canonicalize(dir.path()).expect("canonical root");
        let (window, workspace) = open_workspace(cx, root.clone());
        let window = window.into();
        let active = |cx: &mut TestAppContext| cx.update(|cx| workspace.read(cx).active_file(cx));

        press(cx, window, "ctrl-p");
        type_text(cx, window, "README");
        press(cx, window, "enter");
        assert_eq!(active(cx), Some(root.join("README.md")));

        type_text(cx, window, "edit");
        press(cx, window, "ctrl-w");
        assert_eq!(
            active(cx),
            Some(root.join("README.md")),
            "dirty tab closed without asking"
        );

        press(cx, window, "escape");
        press(cx, window, "ctrl-s");
        press(cx, window, "ctrl-w");
        assert_eq!(active(cx), None);
        let saved = std::fs::read_to_string(root.join("README.md")).expect("read");
        assert!(saved.contains("edit"), "{saved}");
    }
}
