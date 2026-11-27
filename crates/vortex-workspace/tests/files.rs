//! UI integration tests: editing files and managing them from the tree.

#[cfg(test)]
#[path = "common/project.rs"]
mod project;

#[cfg(test)]
mod files {
    use std::time::Duration;

    use gpui_kit::test::{TestAppContextExt, TestQueryExt, TestWindowExt};
    use gpui_kit::{AnyWindowHandle, AppContext as _, TestAppContext};

    use super::project::*;

    const WAIT: Duration = Duration::from_secs(5);

    fn open_readme(cx: &mut TestAppContext, window: AnyWindowHandle) {
        click(cx, window, "activity-files");
        click_label(cx, window, "README.md");
    }

    #[gpui_kit::test]
    fn undoing_back_to_the_saved_text_leaves_nothing_to_save(cx: &mut TestAppContext) {
        let dir = project();
        let (window, workspace) = open_workspace(cx, dir.path().to_path_buf());
        let window = window.into();
        open_readme(cx, window);
        type_text(cx, window, "edit");
        press(cx, window, &primary("w"));
        assert!(has(cx, window, "cancel-close"), "it asks");
        click(cx, window, "cancel-close");
        assert!(has(cx, window, "editor-pane"), "kept open");

        press(cx, window, &primary("z"));
        press(cx, window, &primary("w"));
        assert!(!has(cx, window, "cancel-close"));
        assert_eq!(cx.update(|cx| workspace.read(cx).active_file(cx)), None);
    }

    #[gpui_kit::test]
    async fn closing_saves_or_discards_as_answered(cx: &mut TestAppContext) {
        let dir = project();
        let path = dir.path().join("README.md");
        let (window, workspace) = open_workspace(cx, dir.path().to_path_buf());
        let window = window.into();
        open_readme(cx, window);
        type_text(cx, window, "lost");
        press(cx, window, &primary("w"));
        click(cx, window, "discard-changes");
        assert_eq!(cx.update(|cx| workspace.read(cx).active_file(cx)), None);
        let kept = std::fs::read_to_string(&path).expect("read");
        assert!(!kept.contains("lost"), "{kept}");

        click_label(cx, window, "README.md");
        type_text(cx, window, "kept");
        press(cx, window, &primary("w"));
        click(cx, window, "save-changes");
        cx.wait_for(window, WAIT, |window, _| {
            window.try_find("editor-pane").is_none()
        })
        .await;
        let saved = std::fs::read_to_string(&path).expect("read");
        assert!(saved.contains("kept"), "{saved}");
    }

    #[gpui_kit::test]
    async fn a_changed_file_shows_its_changes_and_a_minimap(cx: &mut TestAppContext) {
        let dir = changed_project();
        let (window, _) = open_workspace(cx, dir.path().to_path_buf());
        let window = window.into();
        open_readme(cx, window);
        assert!(has(cx, window, "minimap"));
        // The button is off until the changes are found.
        for _ in 0..100 {
            cx.executor().timer(Duration::from_millis(50)).await;
            click(cx, window, "editor-toggle-changes");
            if has(cx, window, "editor-changes") {
                break;
            }
        }
        assert!(has(cx, window, "editor-changes"));
        assert!(!has(cx, window, "minimap"));
        click(cx, window, "editor-toggle-changes");
        assert!(has(cx, window, "minimap"));
    }

    #[gpui_kit::test]
    async fn a_file_changed_on_disk_is_read_again(cx: &mut TestAppContext) {
        let dir = project();
        let path = dir.path().join("README.md");
        let (window, _) = open_workspace(cx, dir.path().to_path_buf());
        let window = window.into();
        open_readme(cx, window);
        std::fs::write(&path, "from an agent\n").expect("write");
        click(cx, window, "files-refresh");
        cx.executor().timer(Duration::from_millis(500)).await;
        cx.run_until_parked();
        click_label(cx, window, "Editor README.md");
        type_text(cx, window, "x");
        press(cx, window, &primary("s"));
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(
            text.contains("from an agent") && text.contains('x'),
            "{text}"
        );
    }

    #[gpui_kit::test]
    async fn the_tree_creates_files_and_collapses(cx: &mut TestAppContext) {
        let dir = project();
        let root = std::fs::canonicalize(dir.path()).expect("canonical root");
        let (window, workspace) = open_workspace(cx, root.clone());
        let window = window.into();
        let active = |cx: &mut TestAppContext| cx.update(|cx| workspace.read(cx).active_file(cx));
        click(cx, window, "activity-files");
        click_label(cx, window, "src");
        click(cx, window, "files-new-file");
        type_text(cx, window, "main.rs");
        press(cx, window, "enter");
        assert!(has(cx, window, "name-prompt-submit"), "the name is taken");
        type_text(cx, window, "2");
        click(cx, window, "name-prompt-submit");
        assert!(root.join("src/main.rs2").is_file());
        assert_eq!(active(cx), Some(root.join("src/main.rs2")));

        cx.wait_for(window, WAIT, |window, _| {
            window.find_all_by_label("main.rs2").len() == 2
        })
        .await;
        click(cx, window, "files-collapse-all");
        let shown = cx
            .update_window(window, |_, window, cx| {
                window.render_frame(cx);
                window.try_find_by_label("main.rs").is_some()
            })
            .expect("window");
        assert!(!shown, "folders are closed");
    }

    fn tree_width(cx: &mut TestAppContext, window: AnyWindowHandle) -> f32 {
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            f32::from(window.find("files-tree").bounds().size.width)
        })
        .expect("window")
    }

    #[gpui_kit::test]
    fn the_tree_keeps_its_width_when_the_panel_closes(cx: &mut TestAppContext) {
        let dir = project();
        let (window, _) = open_workspace(cx, dir.path().to_path_buf());
        let window = window.into();
        click(cx, window, "activity-files");
        let before = tree_width(cx, window);
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            let tree = window.find("files-tree").bounds();
            let from = gpui_kit::point(tree.right(), tree.center().y);
            window.drag(
                from,
                gpui_kit::point(from.x + gpui_kit::px(80.), from.y),
                cx,
            );
        })
        .expect("window");
        cx.run_until_parked();
        let sized = tree_width(cx, window);
        assert!(sized > before + 40., "dragged from {before} to {sized}");

        click(cx, window, "activity-files");
        assert!(!has(cx, window, "files-tree"), "closed");
        click(cx, window, "activity-files");
        let reopened = tree_width(cx, window);
        assert!(
            (reopened - sized).abs() < 1.,
            "{sized} came back as {reopened}"
        );
    }
}
