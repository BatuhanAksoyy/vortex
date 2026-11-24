//! Stored conversations: picked on a chat's conversations page, one takes
//! the chat's place, and "New conversation" starts over.

// Inside a `cfg(test)` module so helpers may `expect`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod sessions {
    use gpui_kit::test::{TestQueryExt, TestWindowExt};
    use gpui_kit::{AnyWindowHandle, AppContext as _, TestAppContext};

    use super::common::{Stored, chats, click, labels, no_accounts, open_in, pick, spawned};

    fn open(cx: &mut TestAppContext, backend: Stored) -> AnyWindowHandle {
        open_in(
            cx,
            backend,
            None,
            no_accounts(),
            Some(env!("CARGO_MANIFEST_DIR").into()),
        )
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

    fn top(cx: &mut TestAppContext, window: AnyWindowHandle, id: &'static str) -> f32 {
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            f32::from(window.find(id).bounds().origin.y)
        })
        .expect("window")
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

    /// `first` in the first tab and `second` in a tab beside it, shown.
    fn two_tabs(cx: &mut TestAppContext, window: AnyWindowHandle, first: &str, second: &str) {
        pick(cx, window, first);
        click(cx, window, "chat-new-tab");
        pick(cx, window, second);
    }

    #[gpui_kit::test]
    fn the_conversations_page_lists_the_stored_sessions(cx: &mut TestAppContext) {
        let window = open(cx, Stored::default());
        assert_eq!(
            labels(cx, window, "Fix the build"),
            0,
            "no list beside the chat"
        );
        click(cx, window, "chat-title");
        assert_eq!(labels(cx, window, "Fix the build"), 1);
        assert_eq!(labels(cx, window, "Plan the release"), 1);
        assert!(
            top(cx, window, "conversation-s-new") < top(cx, window, "conversation-s-old"),
            "newest first"
        );
    }

    #[gpui_kit::test]
    fn a_stored_session_opens_in_the_chat_and_resumes(cx: &mut TestAppContext) {
        let backend = Stored::default();
        let window = open(cx, backend.clone());
        assert_eq!(last_resume(&backend), None);
        pick(cx, window, "s-old");
        assert_eq!(last_resume(&backend).as_deref(), Some("s-old"));
        assert!(labels(cx, window, "Plan the release") > 0, "shown");
    }

    #[gpui_kit::test]
    fn new_conversation_starts_over(cx: &mut TestAppContext) {
        let backend = Stored::default();
        let window = open(cx, backend.clone());
        pick(cx, window, "s-new");
        let started = spawned(&backend);
        click(cx, window, "chat-new");
        assert_eq!(spawned(&backend), started + 1);
        assert_eq!(last_resume(&backend), None);
        assert_eq!(labels(cx, window, "Fix the build"), 0, "gone");
    }

    #[gpui_kit::test]
    fn a_session_open_in_another_window_continues_as_a_copy(cx: &mut TestAppContext) {
        let backend = Stored::default();
        let window = open(cx, backend.clone());
        click(cx, window, "chat-title");
        assert_eq!(labels(cx, window, "Open in another window"), 1);
        click(cx, window, "conversation-s-old");
        assert!(!last_forks(&backend), "not open elsewhere: resumed itself");
        pick(cx, window, "s-busy");
        assert_eq!(last_resume(&backend).as_deref(), Some("s-busy"));
        assert!(last_forks(&backend));
    }

    #[gpui_kit::test]
    fn another_conversation_takes_the_chats_place(cx: &mut TestAppContext) {
        let backend = Stored::default();
        let window = open(cx, backend.clone());
        assert_eq!(spawned(&backend), 1, "the first chat");
        pick(cx, window, "s-new");
        assert_eq!(spawned(&backend), 2, "restarted to resume it");
        pick(cx, window, "s-old");
        assert_eq!(spawned(&backend), 3);
        assert_eq!(last_resume(&backend).as_deref(), Some("s-old"));
        assert_eq!(labels(cx, window, "Fix the build"), 0, "replaced");
        assert!(!tab_bar_has(cx, window, ("close-tab", 0)), "still one tab");
    }

    #[gpui_kit::test]
    fn an_open_conversation_is_shown_not_opened_again(cx: &mut TestAppContext) {
        let backend = Stored::default();
        let window = open(cx, backend.clone());
        two_tabs(cx, window, "s-new", "s-old");
        let started = spawned(&backend);
        pick(cx, window, "s-new");
        assert_eq!(spawned(&backend), started);
        assert!(
            tab_bar_has(cx, window, ("close-tab", 1)),
            "the other tab kept its own"
        );
        let shown = cx
            .update_window(window, |_, window, cx| {
                window.render_frame(cx);
                window
                    .find_all_by_label("Fix the build")
                    .iter()
                    .any(gpui_kit::test::ElementSnapshot::visible)
            })
            .expect("window");
        assert!(shown, "its tab is shown");
    }

    #[gpui_kit::test]
    fn a_running_chat_keeps_its_conversation_and_opens_another_beside_it(cx: &mut TestAppContext) {
        let backend = Stored::default();
        let window = open(cx, backend.clone());
        pick(cx, window, "s-old");
        click(cx, window, "chat-composer");
        cx.update_window(window, |_, window, cx| window.input("go on", cx))
            .expect("window");
        cx.run_until_parked();
        cx.update_window(window, |_, window, cx| window.press("enter", cx))
            .expect("window");
        cx.run_until_parked();
        pick(cx, window, "s-new");
        assert!(tab_bar_has(cx, window, ("close-tab", 1)), "a second tab");
        assert_eq!(last_resume(&backend).as_deref(), Some("s-new"));
        click(cx, window, "chat-title");
        assert_eq!(
            labels(cx, window, "Running"),
            1,
            "the other tab still works"
        );
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

    #[gpui_kit::test]
    fn a_closed_tab_ends_its_conversation_but_the_last_one_stays(cx: &mut TestAppContext) {
        let backend = Stored::default();
        let window = open(cx, backend.clone());
        assert!(!tab_bar_has(cx, window, ("close-tab", 0)), "the last tab");
        two_tabs(cx, window, "s-new", "s-old");
        cx.update_window(window, |_, window, cx| {
            window.within("tab-bar").click(("close-tab", 1_usize), cx);
        })
        .expect("window");
        cx.run_until_parked();
        assert!(!tab_bar_has(cx, window, ("close-tab", 0)), "one tab left");
        assert!(
            labels(cx, window, "Fix the build") > 0,
            "the other tab shows"
        );
        let started = spawned(&backend);
        pick(cx, window, "s-old");
        assert_eq!(spawned(&backend), started + 1, "reopened");
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
        two_tabs(cx, window, "s-new", "s-old");
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
