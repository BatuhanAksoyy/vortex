//! UI integration tests: real keystrokes through the production keymap.

// Inside a `cfg(test)` module so helpers may `expect`, like unit tests.
#[cfg(test)]
#[path = "common/project.rs"]
mod project;

#[cfg(test)]
mod workspace {
    use std::time::Duration;

    use gpui_kit::test::{TestAppContextExt, TestQueryExt, TestWindowExt};
    use gpui_kit::{AppContext as _, TestAppContext, px};

    use super::project::*;
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

    #[gpui_kit::test]
    fn command_palette_runs_the_selected_command(cx: &mut TestAppContext) {
        let dir = project();
        let (window, _) = open_workspace(cx, dir.path().to_path_buf());
        let window = window.into();
        assert!(!has(cx, window, "quick-open"));

        press(cx, window, &primary("shift-p"));
        type_text(cx, window, "Go to File");
        press(cx, window, "enter");

        assert!(has(cx, window, "quick-open"));
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
        assert!(!has(cx, window, "editor-pane"));
        assert!(has(cx, window, "files-empty"), "the page says what to do");
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

    #[gpui_kit::test]
    fn the_activity_bar_shows_a_page_beside_the_chat(cx: &mut TestAppContext) {
        let dir = project();
        let (window, _) = open_workspace(cx, dir.path().to_path_buf());
        let window = window.into();
        assert!(has(cx, window, "chat"));
        assert!(!has(cx, window, "side-panel"));
        click(cx, window, "activity-files");
        assert!(has(cx, window, "files-tree"));
        assert!(has(cx, window, "chat"), "beside it");
        click(cx, window, "activity-git");
        assert!(has(cx, window, "git-page"));
        assert!(!has(cx, window, "files-tree"));
        click(cx, window, "side-maximize");
        assert!(!has(cx, window, "chat"), "the panel fills the window");
        click(cx, window, "side-maximize");
        assert!(has(cx, window, "chat"));
        click(cx, window, "activity-git");
        assert!(!has(cx, window, "side-panel"), "closed by its own button");
    }

    #[gpui_kit::test]
    fn a_panel_closed_while_it_fills_the_window_reopens_filling_it(cx: &mut TestAppContext) {
        let dir = project();
        let (window, workspace) = open_workspace(cx, dir.path().to_path_buf());
        let window = window.into();
        let show_chat = |cx: &mut TestAppContext| {
            cx.update_window(window, |_, window, cx| {
                workspace.update(cx, |workspace, cx| workspace.show_chat(window, cx));
            })
            .expect("window");
        };
        click(cx, window, "activity-git");
        click(cx, window, "side-maximize");
        click(cx, window, "side-close");
        assert!(has(cx, window, "chat"));
        show_chat(cx);
        click(cx, window, "activity-git");
        assert!(has(cx, window, "side-panel"));
        assert!(!has(cx, window, "chat"), "it fills the window again");
        show_chat(cx);
        assert!(has(cx, window, "chat"), "the open panel makes room");
    }

    #[gpui_kit::test]
    fn the_file_tree_opens_a_file(cx: &mut TestAppContext) {
        let dir = project();
        let root = std::fs::canonicalize(dir.path()).expect("canonical root");
        let (window, workspace) = open_workspace(cx, root.clone());
        let window = window.into();
        press(cx, window, &primary("shift-e"));
        assert!(has(cx, window, "files-empty"));
        click_label(cx, window, "src");
        click_label(cx, window, "main.rs");
        let active = cx.update(|cx| workspace.read(cx).active_file(cx));
        assert_eq!(active, Some(root.join("src/main.rs")));
        assert!(!has(cx, window, "files-empty"));
    }

    #[gpui_kit::test]
    async fn the_file_tree_marks_changes(cx: &mut TestAppContext) {
        let dir = changed_project();
        let (window, _) = open_workspace(cx, dir.path().to_path_buf());
        let window = window.into();
        click(cx, window, "activity-files");
        cx.wait_for(window, Duration::from_secs(5), |window, _| {
            window.try_find("file-mark-README.md").is_some()
        })
        .await;
        assert!(has(cx, window, "file-mark-src"));
        click_label(cx, window, "src");
        assert!(has(cx, window, "file-mark-src/main.rs"));
        // Listing the files again keeps the folder open.
        std::fs::write(dir.path().join("new.txt"), "new\n").expect("write");
        click(cx, window, "files-refresh");
        cx.wait_for(window, Duration::from_secs(5), |window, _| {
            window.try_find("file-mark-new.txt").is_some()
        })
        .await;
        assert!(has(cx, window, "file-mark-src/main.rs"));
    }

    #[gpui_kit::test]
    fn the_side_panel_is_hidden_and_folds_its_list(cx: &mut TestAppContext) {
        let dir = project();
        let (window, _) = open_workspace(cx, dir.path().to_path_buf());
        let window = window.into();
        click(cx, window, "activity-files");
        click(cx, window, "side-toggle-list");
        assert!(!has(cx, window, "files-tree"), "folded");
        assert!(has(cx, window, "files-empty"));
        click(cx, window, "side-toggle-list");
        assert!(has(cx, window, "files-tree"));
        press(cx, window, &primary("b"));
        assert!(!has(cx, window, "activity-bar"));
        assert!(!has(cx, window, "side-panel"));
        click(cx, window, "toggle-side-panel");
        assert!(has(cx, window, "activity-bar"));
        assert!(has(cx, window, "files-tree"), "open as it was");
        click(cx, window, "side-close");
        assert!(!has(cx, window, "side-panel"));
    }

    #[gpui_kit::test]
    fn the_browser_opens_on_the_chats_right(cx: &mut TestAppContext) {
        // As by default, whatever the user's own settings say.
        let dir = laid_out(r#"{ "side_panel": "left", "browser": "right" }"#);
        let (window, _) = open_workspace(cx, dir.path().to_path_buf());
        let window = window.into();
        click(cx, window, "activity-files");
        click(cx, window, "toggle-browser");
        let lefts = cx
            .update_window(window, |_, window, cx| {
                window.render_frame(cx);
                ["side-panel", "chat", "browser"].map(|id| window.find(id).bounds().left())
            })
            .expect("window");
        assert!(lefts[0] < lefts[1] && lefts[1] < lefts[2], "{lefts:?}");
        assert!(has(cx, window, "browser-notice"));
        click(cx, window, "browser-phone");
        click(cx, window, "browser-close");
        assert!(!has(cx, window, "browser"));
        assert!(has(cx, window, "side-panel"));
    }

    /// A project whose own settings set `layout`.
    fn laid_out(layout: &str) -> tempfile::TempDir {
        let dir = project();
        std::fs::create_dir_all(dir.path().join(".vortex")).expect("mkdir");
        let settings = format!("{{ \"layout\": {layout} }}");
        std::fs::write(dir.path().join(".vortex/settings.json"), settings).expect("write");
        dir
    }

    fn bounds(
        cx: &mut TestAppContext,
        window: gpui_kit::AnyWindowHandle,
        ids: [&'static str; 3],
    ) -> [gpui_kit::Bounds<gpui_kit::Pixels>; 3] {
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            ids.map(|id| window.find(id).bounds())
        })
        .expect("window")
    }

    #[gpui_kit::test]
    fn the_side_panel_and_browser_go_where_the_settings_put_them(cx: &mut TestAppContext) {
        let dir = laid_out(r#"{ "side_panel": "right", "browser": "left" }"#);
        let (window, _) = open_workspace(cx, dir.path().to_path_buf());
        let window = window.into();
        click(cx, window, "activity-files");
        click(cx, window, "toggle-browser");
        let [browser, chat, side] = bounds(cx, window, ["browser", "chat", "side-panel"]);
        assert!(browser.left() < chat.left() && chat.left() < side.left());
        let [_, side, bar] = bounds(cx, window, ["chat", "side-panel", "activity-bar"]);
        assert!(side.left() < bar.left(), "the bar is on the window's edge");
        let [browser, _, side] =
            bounds(cx, window, ["toggle-browser", "chat", "toggle-side-panel"]);
        let middle = chat.center().x;
        assert!(
            browser.right() < middle && middle < side.left(),
            "each on its side"
        );
    }

    #[gpui_kit::test]
    fn the_browser_can_sit_under_the_chats(cx: &mut TestAppContext) {
        let dir = laid_out(r#"{ "browser": "bottom", "title_bar_buttons": false }"#);
        let (window, workspace) = open_workspace(cx, dir.path().to_path_buf());
        let window = window.into();
        assert!(!has(cx, window, "toggle-browser"), "hidden by the settings");
        cx.update_window(window, |_, window, cx| {
            workspace.update(cx, |workspace, cx| workspace.toggle_browser(window, cx));
        })
        .expect("window");
        let [chat, browser, _] = bounds(cx, window, ["chat", "browser", "activity-bar"]);
        assert!(chat.bottom() <= browser.top(), "{chat:?} {browser:?}");
    }

    #[gpui_kit::test]
    fn the_gear_and_cmd_comma_open_the_settings_page(cx: &mut TestAppContext) {
        let dir = project();
        let (window, _) = open_workspace(cx, dir.path().to_path_buf());
        let window = window.into();
        assert!(has(cx, window, "activity-settings"));
        press(cx, window, &primary(","));
        assert!(has(cx, window, "settings-page"));
        assert!(
            has(cx, window, "chat"),
            "beside the work, which shows each change"
        );
        let [bar, page, chat] = bounds(cx, window, ["activity-bar", "settings-page", "chat"]);
        assert!(
            bar.left() < page.left() && page.right() <= chat.left(),
            "by the gear"
        );
        press(cx, window, "escape");
        assert!(!has(cx, window, "settings-page"));
        assert!(has(cx, window, "chat"));
    }

    #[gpui_kit::test]
    fn split_editor_moves_the_file_beside_the_editor(cx: &mut TestAppContext) {
        let dir = project();
        let root = std::fs::canonicalize(dir.path()).expect("canonical root");
        let (window, workspace) = open_workspace(cx, root.clone());
        let window = window.into();
        press(cx, window, &primary("shift-e"));
        click_label(cx, window, "src");
        click_label(cx, window, "main.rs");
        press(cx, window, &primary("\\"));
        assert!(has(cx, window, "files-aside"), "the editor is split");
        assert!(
            has(cx, window, "files-empty"),
            "the first pane is left empty"
        );
        let active = cx.update(|cx| workspace.read(cx).active_file(cx));
        assert_eq!(
            active,
            Some(root.join("src/main.rs")),
            "and the file beside it"
        );
        click_label(cx, window, "README.md");
        let active = cx.update(|cx| workspace.read(cx).active_file(cx));
        assert_eq!(active, Some(root.join("README.md")));
    }
}
