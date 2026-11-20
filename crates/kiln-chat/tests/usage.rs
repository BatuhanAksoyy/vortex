//! The header's account badge and usage meter, and the send lock while a
//! usage limit is reached.

// Inside a `cfg(test)` module so helpers may `expect`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod usage {
    use std::time::{Duration, SystemTime};

    use gpui_kit::TestAppContext;
    use gpui_kit::test::{TestQueryExt, TestWindowExt};
    use kiln_agent::{
        Account, AgentEvent, Limited, PermissionMode, SessionCommand, SessionInfo, Usage,
        UsageWindow,
    };
    use kiln_chat::Status;

    use super::common::Harness;

    const HOUR: Duration = Duration::from_secs(3600);

    fn ready() -> AgentEvent {
        AgentEvent::Ready(SessionInfo {
            models: Vec::new(),
            mode: PermissionMode::AskBeforeEdits,
            account: Some(Account {
                email: "dev@example.com".into(),
                organization: "Example Org".into(),
                plan: "Claude Pro".into(),
            }),
            commands: Vec::new(),
        })
    }

    fn usage(session: u8, limited_for: Option<Duration>) -> AgentEvent {
        let now = SystemTime::now();
        AgentEvent::Usage(Usage {
            session: Some(UsageWindow {
                percent: session,
                resets_at: Some(now + HOUR),
            }),
            weekly: Some(UsageWindow {
                percent: 19,
                resets_at: Some(now + 50 * HOUR),
            }),
            limited: limited_for.map(|wait| Limited {
                until: Some(now + wait),
            }),
        })
    }

    #[gpui_kit::test]
    fn the_header_shows_the_account_and_its_usage(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.emit(cx, ready());
        assert!(harness.commands().contains(&SessionCommand::RefreshUsage));
        harness.emit(cx, usage(42, None));
        harness.with_window(cx, |window, _| {
            assert!(window.find_by_label("dev@example.com · Pro").visible());
            assert!(window.find_by_label("42% used").visible());
        });
        harness.commands();
        harness.with_window(cx, |window, cx| window.click("chat-usage", cx));
        assert_eq!(harness.commands(), [SessionCommand::RefreshUsage]);
        harness.with_window(cx, |window, _| {
            assert!(window.find("chat-usage-panel").visible());
            assert!(window.find("chat-usage-updated").visible());
        });
    }

    #[gpui_kit::test]
    fn usage_is_refreshed_while_the_chat_sits_idle(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.emit(cx, ready());
        harness.commands();
        cx.executor().advance_clock(Duration::from_mins(5));
        cx.run_until_parked();
        assert_eq!(harness.commands(), [SessionCommand::RefreshUsage]);
    }

    #[gpui_kit::test]
    fn a_reached_limit_stops_sending_and_says_when_it_resets(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.emit(cx, ready());
        harness.commands();
        harness.emit(cx, usage(100, Some(HOUR)));
        harness.type_and_send(cx, "hi");
        assert_eq!(harness.commands(), []);
        assert_eq!(harness.status(cx), Status::Idle);
        harness.with_window(cx, |window, _| {
            assert!(window.find("chat-limit").visible());
        });
    }

    #[gpui_kit::test]
    fn sending_works_again_once_the_limit_resets(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.emit(cx, ready());
        harness.commands();
        harness.emit(cx, usage(100, Some(HOUR)));
        cx.executor().advance_clock(HOUR + Duration::from_secs(1));
        cx.run_until_parked();
        assert!(harness.commands().contains(&SessionCommand::RefreshUsage));
        assert!(!cx.update(|cx| harness.view.read(cx).is_limited()));
        harness.with_window(cx, |window, _| {
            assert!(window.try_find("chat-limit").is_none());
        });
        harness.type_and_send(cx, "hi");
        assert_eq!(harness.commands(), [SessionCommand::Send("hi".into())]);
    }
}
