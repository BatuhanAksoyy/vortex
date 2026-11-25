//! A recap is asked for once the user has been away a while.

// Inside a `cfg(test)` module so helpers may `expect`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod recap {
    use std::time::Duration;

    use gpui_kit::{TestAppContext, VisualTestContext};
    use kiln_agent::{AgentEvent, SessionCommand, TurnOutcome};
    use kiln_chat::Status;

    use super::common::Harness;

    fn turn(harness: &Harness, cx: &mut TestAppContext) {
        harness.type_and_send(cx, "go on");
        harness.emit(cx, AgentEvent::TurnEnded(TurnOutcome::Completed));
    }

    fn come_back(harness: &Harness, cx: &mut TestAppContext) {
        harness.with_window(cx, |window, _| window.activate_window());
    }

    fn leave(harness: &Harness, cx: &mut TestAppContext) {
        VisualTestContext::from_window(harness.window, cx).deactivate_window();
        cx.run_until_parked();
    }

    fn wait(cx: &mut TestAppContext, minutes: u64) {
        cx.executor().advance_clock(Duration::from_mins(minutes));
        cx.run_until_parked();
    }

    fn recaps(harness: &Harness) -> usize {
        let asked = |command: &SessionCommand| *command == SessionCommand::Recap;
        harness.commands().iter().filter(|it| asked(it)).count()
    }

    #[gpui_kit::test]
    fn one_is_asked_for_after_a_while_away(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        come_back(&harness, cx);
        for _ in 0..3 {
            turn(&harness, cx);
        }
        wait(cx, 4);
        assert_eq!(recaps(&harness), 0, "the user is here");
        leave(&harness, cx);
        wait(cx, 2);
        come_back(&harness, cx);
        wait(cx, 2);
        assert_eq!(recaps(&harness), 0, "came back in time");
        leave(&harness, cx);
        wait(cx, 2);
        wait(cx, 2);
        assert_eq!(recaps(&harness), 1);
        harness.emit(cx, AgentEvent::Recap("All done.".into()));
        wait(cx, 10);
        assert_eq!(recaps(&harness), 0, "one for each turn at most");
        assert_eq!(harness.status(cx), Status::Idle);
    }

    #[gpui_kit::test]
    fn a_short_conversation_gets_none(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        turn(&harness, cx);
        leave(&harness, cx);
        wait(cx, 10);
        assert_eq!(recaps(&harness), 0);
    }

    #[gpui_kit::test]
    fn none_is_asked_for_while_a_turn_runs(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        for _ in 0..3 {
            turn(&harness, cx);
        }
        harness.type_and_send(cx, "one more");
        leave(&harness, cx);
        wait(cx, 10);
        assert_eq!(recaps(&harness), 0);
        harness.emit(cx, AgentEvent::TurnEnded(TurnOutcome::Completed));
        wait(cx, 4);
        assert_eq!(recaps(&harness), 1, "the turn ended while away");
    }
}
