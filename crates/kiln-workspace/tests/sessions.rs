//! The sidebar's session list: stored conversations open in the chat, and
//! "New session" starts over.

// Inside a `cfg(test)` module so helpers may `expect`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod sessions {
    use gpui_kit::test::{TestQueryExt, TestWindowExt};
    use gpui_kit::{AnyWindowHandle, AppContext as _, TestAppContext};

    use super::common::{Stored, chats, click, labels, no_accounts, open_keeping, spawned};

    fn open(cx: &mut TestAppContext, backend: Stored) -> AnyWindowHandle {
        open_keeping(cx, backend, None, no_accounts())
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

    #[gpui_kit::test]
    fn another_conversation_opens_in_a_new_tab(cx: &mut TestAppContext) {
        let backend = Stored::default();
        let window = open(cx, backend.clone());
        assert_eq!(spawned(&backend), 1, "the first chat");
        click(cx, window, "session-s-new");
        assert_eq!(spawned(&backend), 2, "the blank chat takes it");
        click(cx, window, "session-s-old");
        assert_eq!(spawned(&backend), 3, "a tab that starts only its resume");
        assert_eq!(last_resume(&backend).as_deref(), Some("s-old"));
        assert_eq!(
            labels(cx, window, "Plan the release"),
            2,
            "listed and shown"
        );

        cx.update_window(window, |_, window, cx| {
            window.within("tab-bar").click(0_usize, cx);
        })
        .expect("window");
        cx.run_until_parked();
        assert_eq!(
            labels(cx, window, "Fix the build"),
            2,
            "the first tab kept it"
        );
    }

    #[gpui_kit::test]
    fn an_open_conversation_is_shown_not_opened_again(cx: &mut TestAppContext) {
        let backend = Stored::default();
        let window = open(cx, backend.clone());
        click(cx, window, "session-s-new");
        click(cx, window, "session-s-old");
        let started = spawned(&backend);
        click(cx, window, "session-s-new");
        assert_eq!(spawned(&backend), started);
        assert_eq!(labels(cx, window, "Fix the build"), 2);
    }

    #[gpui_kit::test]
    fn split_right_puts_a_new_chat_beside_the_first(cx: &mut TestAppContext) {
        let backend = Stored::default();
        let window = open(cx, backend.clone());
        click(cx, window, "chat-split-right");
        assert_eq!(spawned(&backend), 2);
        let shown = chats(cx, window);
        assert_eq!(shown.len(), 2, "both chats are on screen");
        assert!(shown[0].origin.y == shown[1].origin.y && shown[0].origin.x != shown[1].origin.x);
        let views = cx
            .update_window(window, |_, window, cx| {
                window.render_frame(cx);
                window
                    .find_all("chat")
                    .iter()
                    .map(|chat| chat.path().iter().rev().nth(1).cloned())
                    .collect::<Vec<_>>()
            })
            .expect("window");
        // One view drawn twice would also track its composer's focus twice.
        assert_ne!(views[0], views[1], "a different chat on each side");
    }

    fn tab_bar_has(
        cx: &mut TestAppContext,
        window: AnyWindowHandle,
        id: (&'static str, usize),
    ) -> bool {
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            !window.within("tab-bar").find_all(id).is_empty()
        })
        .expect("window")
    }

    #[gpui_kit::test]
    fn a_closed_tab_ends_its_conversation_but_the_last_one_stays(cx: &mut TestAppContext) {
        let backend = Stored::default();
        let window = open(cx, backend.clone());
        assert!(!tab_bar_has(cx, window, ("close-tab", 0)), "the last tab");
        click(cx, window, "session-s-new");
        click(cx, window, "session-s-old");
        cx.update_window(window, |_, window, cx| {
            window.within("tab-bar").click(("close-tab", 1_usize), cx);
        })
        .expect("window");
        cx.run_until_parked();
        assert!(!tab_bar_has(cx, window, ("close-tab", 0)), "one tab left");
        assert_eq!(
            labels(cx, window, "Fix the build"),
            2,
            "the other tab shows"
        );
        let started = spawned(&backend);
        click(cx, window, "session-s-old");
        assert_eq!(spawned(&backend), started + 1, "reopened in a new tab");
    }

    #[gpui_kit::test]
    fn a_turn_running_in_another_tab_is_marked_in_the_sidebar(cx: &mut TestAppContext) {
        let backend = Stored::default();
        let window = open(cx, backend.clone());
        click(cx, window, "session-s-old");
        click(cx, window, "chat-composer");
        cx.update_window(window, |_, window, cx| window.input("go on", cx))
            .expect("window");
        cx.run_until_parked();
        cx.update_window(window, |_, window, cx| window.press("enter", cx))
            .expect("window");
        cx.run_until_parked();
        click(cx, window, "session-s-new");
        assert_eq!(labels(cx, window, "Fix the build"), 2, "shown in a new tab");
        assert_eq!(
            labels(cx, window, "Running"),
            1,
            "the other tab still works"
        );
    }

    /// Types and sends `text`; whether it was said in the chat on screen.
    fn typed_into_shown_chat(
        cx: &mut TestAppContext,
        window: AnyWindowHandle,
        text: &'static str,
    ) -> bool {
        // Focus moves on from the panel to its chat once a frame is drawn.
        cx.update_window(window, |_, window, cx| window.render_frame(cx))
            .expect("window");
        cx.run_until_parked();
        cx.update_window(window, |_, window, cx| window.input(text, cx))
            .expect("window");
        cx.run_until_parked();
        cx.update_window(window, |_, window, cx| window.press("enter", cx))
            .expect("window");
        cx.run_until_parked();
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            window
                .find_all_by_label(text)
                .iter()
                .any(gpui_kit::test::ElementSnapshot::visible)
        })
        .expect("window")
    }

    #[gpui_kit::test]
    fn choosing_a_tab_or_splitting_puts_the_keyboard_in_that_chat(cx: &mut TestAppContext) {
        let backend = Stored::default();
        let window = open(cx, backend.clone());
        // An inactive window reports no focus changes.
        cx.update_window(window, |_, window, _| window.activate_window())
            .expect("window");
        click(cx, window, "session-s-new");
        click(cx, window, "session-s-old");
        cx.update_window(window, |_, window, cx| {
            window.within("tab-bar").click(0_usize, cx);
        })
        .expect("window");
        cx.run_until_parked();
        assert!(typed_into_shown_chat(cx, window, "first"), "the chosen tab");
        click(cx, window, "chat-split-right");
        assert!(
            typed_into_shown_chat(cx, window, "second"),
            "the new chat beside it"
        );
    }
}
