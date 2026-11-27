//! Up and Down go through the messages sent; Escape stops the turn and
//! hands back a message nothing came in reply to.

// Inside a `cfg(test)` module so helpers may `expect`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod turn_keys {
    use gpui_kit::TestAppContext;
    use gpui_kit::test::{TestQueryExt, TestWindowExt};
    use vortex_agent::{AgentEvent, BlockKey, SessionCommand, TurnOutcome};
    use vortex_chat::Status;

    use super::common::Harness;

    fn press(harness: &Harness, cx: &mut TestAppContext, key: &str) {
        harness.with_window(cx, |window, cx| window.press(key, cx));
    }

    fn reply(harness: &Harness, cx: &mut TestAppContext, text: &str) {
        harness.emit(
            cx,
            AgentEvent::TextDelta {
                block: BlockKey {
                    message_id: text.into(),
                    index: 0,
                },
                text: text.into(),
            },
        );
        harness.emit(cx, AgentEvent::TurnEnded(TurnOutcome::Completed));
    }

    /// Two finished turns, "first" then "second", and an empty box.
    fn two_sent(cx: &mut TestAppContext) -> Harness {
        let harness = Harness::new(cx);
        harness.type_and_send(cx, "first");
        reply(&harness, cx, "one");
        harness.type_and_send(cx, "second");
        reply(&harness, cx, "two");
        harness.commands();
        harness
    }

    fn sent(text: &str) -> SessionCommand {
        SessionCommand::Send(text.into())
    }

    /// The messages sent since the last look, without the refreshes a turn
    /// asks for.
    fn sends(harness: &Harness) -> Vec<SessionCommand> {
        harness
            .commands()
            .into_iter()
            .filter(|command| matches!(command, SessionCommand::Send(_)))
            .collect()
    }

    fn shown(harness: &Harness, cx: &mut TestAppContext, label: &str) -> usize {
        harness.with_window(cx, |window, _| window.find_all_by_label(label).len())
    }

    #[gpui_kit::test]
    fn up_brings_back_the_messages_sent_newest_first(cx: &mut TestAppContext) {
        let harness = two_sent(cx);
        press(&harness, cx, "up");
        press(&harness, cx, "up");
        press(&harness, cx, "enter");
        assert_eq!(sends(&harness), [sent("first")]);
    }

    #[gpui_kit::test]
    fn down_goes_back_toward_the_empty_box(cx: &mut TestAppContext) {
        let harness = two_sent(cx);
        press(&harness, cx, "up");
        press(&harness, cx, "up");
        press(&harness, cx, "down");
        press(&harness, cx, "enter");
        assert_eq!(sends(&harness), [sent("second")]);
        reply(&harness, cx, "again");
        press(&harness, cx, "up");
        press(&harness, cx, "down");
        press(&harness, cx, "enter");
        assert!(sends(&harness).is_empty(), "the box is empty again");
    }

    #[gpui_kit::test]
    fn up_leaves_a_message_being_written_alone(cx: &mut TestAppContext) {
        let harness = two_sent(cx);
        harness.with_window(cx, |window, cx| window.input("draft", cx));
        press(&harness, cx, "up");
        press(&harness, cx, "enter");
        assert_eq!(sends(&harness), [sent("draft")]);
    }

    #[gpui_kit::test]
    fn escape_before_any_reply_hands_the_message_back(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.type_and_send(cx, "make it faster");
        harness.commands();
        press(&harness, cx, "escape");
        assert_eq!(harness.commands(), [SessionCommand::Interrupt]);
        assert_eq!(shown(&harness, cx, "make it faster"), 0, "taken back");
        harness.emit(cx, AgentEvent::TurnEnded(TurnOutcome::Interrupted));
        assert_eq!(shown(&harness, cx, "Interrupted"), 0);
        press(&harness, cx, "enter");
        assert_eq!(sends(&harness), [sent("make it faster")]);
    }

    #[gpui_kit::test]
    fn escape_after_a_reply_keeps_the_message(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.type_and_send(cx, "make it faster");
        harness.emit(
            cx,
            AgentEvent::TextDelta {
                block: BlockKey {
                    message_id: "m".into(),
                    index: 0,
                },
                text: "Looking".into(),
            },
        );
        harness.commands();
        press(&harness, cx, "escape");
        assert_eq!(harness.commands(), [SessionCommand::Interrupt]);
        harness.emit(cx, AgentEvent::TurnEnded(TurnOutcome::Interrupted));
        assert_eq!(shown(&harness, cx, "make it faster"), 1);
        assert_eq!(shown(&harness, cx, "Interrupted"), 1);
        assert_eq!(harness.status(cx), Status::Idle);
    }
}
