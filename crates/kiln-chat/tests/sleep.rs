//! A chat idle long enough lets its process go and starts it again on the
//! same conversation with the next message; work in the background keeps it
//! awake.

// Inside a `cfg(test)` module so helpers may `expect`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod sleep {
    use std::time::Duration;

    use gpui_kit::TestAppContext;
    use kiln_agent::{AgentEvent, SessionCommand, TurnOutcome};
    use kiln_chat::Status;

    use super::common::Harness;

    const AFTER: Duration = Duration::from_mins(30);

    /// A chat that sleeps after `AFTER`, its conversation stored as `s-1`.
    fn started(cx: &mut TestAppContext) -> Harness {
        let harness = Harness::new(cx);
        cx.update(|cx| {
            harness
                .view
                .update(cx, |chat, cx| chat.set_sleep_after(Some(AFTER), cx));
        });
        harness.type_and_send(cx, "start");
        harness.emit(
            cx,
            AgentEvent::Started {
                session_id: "s-1".into(),
                model: "claude-opus-5-5".into(),
                cli_version: None,
            },
        );
        harness.emit(cx, AgentEvent::TurnEnded(TurnOutcome::Completed));
        harness
    }

    fn wait(cx: &mut TestAppContext, time: Duration) {
        cx.executor().advance_clock(time);
        cx.run_until_parked();
    }

    fn asleep(harness: &Harness, cx: &mut TestAppContext) -> bool {
        cx.update(|cx| harness.view.read(cx).is_asleep())
    }

    #[gpui_kit::test]
    fn an_idle_chat_sleeps_and_wakes_on_the_same_conversation(cx: &mut TestAppContext) {
        let harness = started(cx);
        wait(cx, Duration::from_mins(29));
        assert!(!asleep(&harness, cx), "not yet");
        wait(cx, Duration::from_mins(1));
        assert!(asleep(&harness, cx));
        harness.type_and_send(cx, "go on");
        assert_eq!(harness.status(cx), Status::Running);
        let sent = harness.commands();
        assert!(
            matches!(sent.as_slice(), [.., SessionCommand::Send(_)]),
            "{sent:?}"
        );
        let script = harness.script.borrow();
        assert_eq!(script.spawned, 2, "started again");
        let resumed = script
            .last_spec
            .as_ref()
            .and_then(|spec| spec.resume.clone());
        assert_eq!(resumed.as_deref(), Some("s-1"));
    }

    #[gpui_kit::test]
    fn work_in_the_background_keeps_it_awake(cx: &mut TestAppContext) {
        let harness = started(cx);
        harness.emit(cx, AgentEvent::BackgroundTasks(1));
        wait(cx, AFTER * 2);
        assert!(!asleep(&harness, cx), "a shell command still runs");
        harness.emit(cx, AgentEvent::BackgroundTasks(0));
        wait(cx, AFTER);
        assert!(asleep(&harness, cx), "idle since it ended");
    }
}
