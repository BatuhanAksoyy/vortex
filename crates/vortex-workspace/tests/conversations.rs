//! The conversations page a chat opens from its name: searching the
//! stored conversations and switching to one.

// Inside a `cfg(test)` module so helpers may `expect`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod conversations {
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{AnyWindowHandle, AppContext as _, ElementId, TestAppContext};

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

    fn has(cx: &mut TestAppContext, window: AnyWindowHandle, id: &'static str) -> bool {
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            window.try_find(ElementId::Name(id.into())).is_some()
        })
        .expect("window")
    }

    fn last_resume(backend: &Stored) -> Option<String> {
        backend
            .0
            .borrow()
            .last()
            .and_then(|spec| spec.resume.clone())
    }

    #[gpui_kit::test]
    fn the_chats_name_opens_its_conversations_and_switches_to_one(cx: &mut TestAppContext) {
        let backend = Stored::default();
        let window = open(cx, backend.clone());
        pick(cx, window, "s-new");
        click(cx, window, "chat-title");
        assert!(has(cx, window, "conversations-overlay"));
        assert!(has(cx, window, "conversation-s-old"));
        click(cx, window, "conversations-search");
        cx.update_window(window, |_, window, cx| window.input("release", cx))
            .expect("window");
        cx.run_until_parked();
        assert!(has(cx, window, "conversation-s-old"));
        assert!(
            !has(cx, window, "conversation-s-new"),
            "the search hides it"
        );
        let started = spawned(&backend);
        click(cx, window, "conversation-s-old");
        assert!(!has(cx, window, "conversations-overlay"));
        assert_eq!(spawned(&backend), started + 1);
        assert_eq!(last_resume(&backend).as_deref(), Some("s-old"));
        assert!(labels(cx, window, "Plan the release") > 0, "shown");
        assert_eq!(labels(cx, window, "Fix the build"), 0, "in its place");
        assert_eq!(chats(cx, window).len(), 1);
    }

    #[gpui_kit::test]
    fn a_picked_conversation_keeps_its_listed_name(cx: &mut TestAppContext) {
        let backend = Stored::default();
        let window = open(cx, backend);
        pick(cx, window, "s-new");
        pick(cx, window, "s-named");
        assert!(
            labels(cx, window, "Conversation: Ship v2") > 0,
            "named as listed, not after its first message"
        );
        assert_eq!(
            labels(
                cx,
                window,
                "Conversation: Start the work on the next version"
            ),
            0,
            "not its first message"
        );
    }

    #[gpui_kit::test]
    fn escape_or_the_shown_conversation_closes_the_page(cx: &mut TestAppContext) {
        let backend = Stored::default();
        let window = open(cx, backend.clone());
        pick(cx, window, "s-new");
        click(cx, window, "chat-title");
        cx.update_window(window, |_, window, cx| window.press("escape", cx))
            .expect("window");
        cx.run_until_parked();
        assert!(!has(cx, window, "conversations-overlay"));
        click(cx, window, "chat-title");
        let started = spawned(&backend);
        click(cx, window, "conversation-s-new");
        assert!(!has(cx, window, "conversations-overlay"));
        assert_eq!(spawned(&backend), started, "nothing opened again");
    }
}
