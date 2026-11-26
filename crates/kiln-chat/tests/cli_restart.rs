//! Moving a chat onto a newly installed CLI: its process restarts and
//! resumes the conversation, after the turn under way if one runs.

// Inside a `cfg(test)` module so helpers may `expect`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod cli_restart {
    use gpui_kit::TestAppContext;
    use kiln_agent::{AgentEvent, SessionCommand, TurnOutcome};
    use kiln_chat::Status;

    use super::common::Harness;

    fn started(harness: &Harness, cx: &mut TestAppContext) {
        harness.type_and_send(cx, "one");
        harness.emit(
            cx,
            AgentEvent::Started {
                session_id: "s-1".into(),
                model: "claude-opus-5-5".into(),
                cli_version: Some("2.1.311".into()),
            },
        );
    }

    fn restart(harness: &Harness, cx: &mut TestAppContext) {
        let view = harness.view.clone();
        harness.with_window(cx, move |window, cx| {
            view.update(cx, |chat, cx| chat.restart_on_new_cli(window, cx));
        });
    }

    fn spawned(harness: &Harness) -> (usize, Option<String>) {
        let script = harness.script.borrow();
        let resume = script
            .last_spec
            .as_ref()
            .and_then(|spec| spec.resume.clone());
        (script.spawned, resume)
    }

    #[gpui_kit::test]
    fn an_idle_chat_resumes_its_conversation_on_the_new_cli(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        started(&harness, cx);
        harness.emit(cx, AgentEvent::TurnEnded(TurnOutcome::Completed));
        let version = cx.update(|cx| harness.view.read(cx).cli_version().map(str::to_owned));
        assert_eq!(version.as_deref(), Some("2.1.311"));
        let (before, _) = spawned(&harness);
        restart(&harness, cx);
        assert_eq!(spawned(&harness), (before + 1, Some("s-1".into())));
        assert_eq!(
            harness.status(cx),
            Status::Idle,
            "ready for the next message"
        );
    }

    #[gpui_kit::test]
    fn a_running_turn_finishes_before_the_restart(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        started(&harness, cx);
        let (before, _) = spawned(&harness);
        restart(&harness, cx);
        assert_eq!(spawned(&harness).0, before, "the turn is not cut off");
        assert_eq!(harness.status(cx), Status::Running);
        assert!(!harness.commands().contains(&SessionCommand::Interrupt));
        harness.emit(cx, AgentEvent::TurnEnded(TurnOutcome::Completed));
        assert_eq!(spawned(&harness), (before + 1, Some("s-1".into())));
    }
}
