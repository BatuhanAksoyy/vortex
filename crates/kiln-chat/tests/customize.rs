//! The chat's customize mode: its items are moved and hidden, the usage
//! meter reads both windows, and the harness takes another color.

// Inside a `cfg(test)` module so helpers may `expect`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod customize {
    use std::{cell::RefCell, rc::Rc};

    use gpui_kit::TestAppContext;
    use gpui_kit::test::{TestQueryExt, TestWindowExt};
    use kiln_agent::{AgentEvent, Usage, UsageWindow};
    use kiln_chat::{ChatEvent, ChatItem, ChatLook, UsageShown};

    use super::common::Harness;

    /// The looks the chat handed on to keep.
    fn kept(cx: &mut TestAppContext, harness: &Harness) -> Rc<RefCell<Vec<ChatLook>>> {
        let looks = Rc::new(RefCell::new(Vec::new()));
        let kept = looks.clone();
        let keep = move |_, event: &ChatEvent, _: &mut gpui_kit::App| {
            if let ChatEvent::LookChanged(look) = event {
                kept.borrow_mut().push(look.clone());
            }
        };
        cx.update(|cx| cx.subscribe(&harness.view, keep).detach());
        looks
    }

    fn current(cx: &mut TestAppContext) -> ChatLook {
        cx.update(|cx| ChatLook::current(cx).clone())
    }

    #[gpui_kit::test]
    fn items_move_from_the_title_bar_to_the_strip_and_hide(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        let looks = kept(cx, &harness);
        harness.with_window(cx, |window, cx| window.click("chat-customize", cx));
        assert!(current(cx).editing);
        harness.with_window(cx, |window, _| {
            assert!(window.find("chat-look-bar").visible());
        });
        // Along the strip: a test drag jumps in eighths of the way, and a
        // first step over the transcript would never start it.
        harness.with_window(cx, |window, cx| {
            window.drag_to("chat-item-drag-agents", "chat-spot-bottom_start", cx);
        });
        let look = current(cx);
        assert!(!look.bottom_end.contains(&ChatItem::Agents));
        assert!(look.bottom_start.contains(&ChatItem::Agents));
        assert_eq!(looks.borrow().last(), Some(&look), "handed on to keep");

        // Without a drag, from the item's menu.
        harness.with_window(cx, |window, cx| window.click("chat-item-menu-account", cx));
        harness.with_window(cx, |window, cx| {
            let at = window
                .find_by_label("Move to Composer, right")
                .bounds()
                .center();
            window.drag(at, at, cx);
        });
        assert_eq!(current(cx).bottom_end.last(), Some(&ChatItem::Account));

        harness.with_window(cx, |window, cx| window.click("chat-item-menu-new_chat", cx));
        harness.with_window(cx, |window, cx| {
            let at = window.find_by_label("Hide").bounds().center();
            window.drag(at, at, cx);
        });
        assert_eq!(current(cx).hidden(), [ChatItem::NewChat]);
        harness.with_window(cx, |window, _| {
            assert!(window.try_find("chat-new").is_none());
        });
        harness.with_window(cx, |window, cx| {
            window.drag_to("chat-hidden-new_chat", "chat-spot-top", cx);
        });
        assert!(current(cx).top.contains(&ChatItem::NewChat), "shown again");

        harness.with_window(cx, |window, cx| window.click("chat-look-reset", cx));
        harness.with_window(cx, |window, cx| window.click("chat-customize", cx));
        let look = current(cx);
        assert_eq!(look, ChatLook::default());
        harness.with_window(cx, |window, _| {
            assert!(window.try_find("chat-look-bar").is_none());
        });
    }

    #[gpui_kit::test]
    fn the_meter_reads_both_windows_and_the_harness_takes_a_color(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        let window = |percent| {
            Some(UsageWindow {
                percent,
                resets_at: None,
            })
        };
        harness.emit(
            cx,
            AgentEvent::Usage(Usage {
                session: window(12),
                weekly: window(40),
                ..Usage::default()
            }),
        );
        cx.update(|cx| {
            cx.set_global(ChatLook {
                usage: UsageShown::Both,
                ..ChatLook::default()
            });
        });
        harness.with_window(cx, |window, _| {
            assert!(window.find_by_label("Session 12% · Weekly 40%").visible());
        });

        harness.with_window(cx, |window, cx| window.click("chat-customize", cx));
        harness.with_window(cx, |window, cx| window.click("chat-look-color-red", cx));
        assert_eq!(current(cx).claude_color, Some(0xe5_48_4d));
        let red = cx.update(|cx| kiln_chat::Brand::Claude.color(cx));
        assert_eq!(red, gpui_kit::rgb(0xe5_48_4d).into());

        // The strip still sends while it is being customized.
        harness.commands();
        harness.type_and_send(cx, "hello");
        assert_eq!(harness.status(cx), kiln_chat::Status::Running);
        assert_ne!(harness.commands(), [] as [kiln_agent::SessionCommand; 0]);
    }
}
