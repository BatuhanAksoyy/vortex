//! Choosing the folder a session works in, and the side panel following the
//! active session.

#[cfg(test)]
mod common;

#[cfg(test)]
mod projects {
    use std::path::{Path, PathBuf};

    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{AnyWindowHandle, AppContext as _, TestAppContext};

    use super::common::{Stored, chats, click, labels, no_accounts, open_in, pick, spawned};

    /// Two project folders, `one` and `two`, and their parent.
    fn folders() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path().canonicalize().expect("canonical");
        let [one, two] = ["one", "two"].map(|name| root.join(name));
        for folder in [&one, &two] {
            std::fs::create_dir(folder).expect("mkdir");
            std::fs::write(folder.join("readme.md"), "# demo\n").expect("write");
        }
        (dir, one, two)
    }

    fn has(cx: &mut TestAppContext, window: AnyWindowHandle, id: &'static str) -> bool {
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            window.try_find(id).is_some()
        })
        .expect("window")
    }

    /// Opens `folder` from the projects page of the chat shown.
    fn choose(cx: &mut TestAppContext, window: AnyWindowHandle, folder: &Path) {
        click(cx, window, "chat-project");
        browse(cx, window, folder);
    }

    /// Chooses `folder` in the system's dialog.
    fn browse(cx: &mut TestAppContext, window: AnyWindowHandle, folder: &Path) {
        click(cx, window, "projects-browse");
        let chosen = folder.to_path_buf();
        cx.simulate_path_prompt_response(move |_| Some(vec![chosen]));
        cx.run_until_parked();
    }

    fn click_tab(cx: &mut TestAppContext, window: AnyWindowHandle, tab: usize) {
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            window.within("tab-bar").click(tab, cx);
        })
        .expect("window");
        cx.run_until_parked();
    }

    fn close_tab(cx: &mut TestAppContext, window: AnyWindowHandle, tab: usize) {
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            window.within("tab-bar").click(("close-tab", tab), cx);
        })
        .expect("window");
        cx.run_until_parked();
    }

    fn folder_of_last(backend: &Stored) -> Option<PathBuf> {
        let specs = backend.0.borrow();
        specs.last().map(|spec| spec.cwd.clone())
    }

    #[gpui_kit::test]
    fn without_a_folder_kiln_asks_for_one(cx: &mut TestAppContext) {
        let (_dir, one, _) = folders();
        let backend = Stored::default();
        let window = open_in(cx, backend.clone(), None, no_accounts(), None);
        assert!(has(cx, window, "start-page"));
        assert!(!has(cx, window, "activity-bar"), "nothing to show there");
        assert_eq!(spawned(&backend), 0);
        browse(cx, window, &one);
        assert!(!has(cx, window, "start-page"));
        assert_eq!(chats(cx, window).len(), 1);
        assert_eq!(folder_of_last(&backend), Some(one));
    }

    #[gpui_kit::test]
    fn a_blank_session_moves_and_a_used_one_opens_a_tab(cx: &mut TestAppContext) {
        let (_dir, one, two) = folders();
        let backend = Stored::default();
        let window = open_in(cx, backend.clone(), None, no_accounts(), Some(one.clone()));
        choose(cx, window, &two);
        assert_eq!(folder_of_last(&backend), Some(two));

        pick(cx, window, "s-new");
        choose(cx, window, &one);
        assert_eq!(folder_of_last(&backend), Some(one));
        assert!(
            has(cx, window, "chat-empty"),
            "a new session, the used one is kept"
        );
        assert_eq!(spawned(&backend), 4);
    }

    #[gpui_kit::test]
    fn the_side_panel_shows_the_active_sessions_folder(cx: &mut TestAppContext) {
        let (_dir, one, two) = folders();
        let window = open_in(cx, Stored::default(), None, no_accounts(), Some(one));
        pick(cx, window, "s-new");
        click(cx, window, "activity-files");
        assert_eq!(labels(cx, window, "Showing one"), 1);
        choose(cx, window, &two);
        assert_eq!(labels(cx, window, "Showing two"), 1);
        click_tab(cx, window, 0);
        assert_eq!(labels(cx, window, "Showing one"), 1);
        click_tab(cx, window, 1);
        assert_eq!(labels(cx, window, "Showing two"), 1);
        close_tab(cx, window, 1);
        assert_eq!(labels(cx, window, "Showing one"), 1, "the tab left");
    }

    #[gpui_kit::test]
    fn sessions_come_back_in_their_folders(cx: &mut TestAppContext) {
        let (dir, one, two) = folders();
        let layouts = dir.path().join("layouts.json");
        let kept = Some(layouts.clone());
        let before = open_in(
            cx,
            Stored::default(),
            kept,
            no_accounts(),
            Some(one.clone()),
        );
        pick(cx, before, "s-new");
        choose(cx, before, &two);
        pick(cx, before, "s-old");

        let backend = Stored::default();
        open_in(
            cx,
            backend.clone(),
            Some(layouts.clone()),
            no_accounts(),
            None,
        );
        let resumed: Vec<_> = {
            let specs = backend.0.borrow();
            let resumed = specs.iter().map(|s| (s.resume.clone(), s.cwd.clone()));
            resumed.collect()
        };
        let expected = [("s-new", &one), ("s-old", &two)]
            .map(|(id, folder)| (Some(id.to_owned()), folder.clone()));
        assert_eq!(resumed, expected);

        // A folder that has a tab shows it instead of opening another.
        let again = Stored::default();
        open_in(cx, again.clone(), Some(layouts), no_accounts(), Some(one));
        assert_eq!(spawned(&again), 2);
    }
}
