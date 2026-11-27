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
    use vortex_agent::{
        Account, AgentEvent, Limited, PermissionMode, PromptTokens, SessionCommand, SessionInfo,
        TurnOutcome, Usage, UsageWindow,
    };
    use vortex_chat::Status;

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
            long_window_days: None,
        })
    }

    #[gpui_kit::test]
    fn the_meter_shows_the_window_nearer_its_limit(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.emit(cx, ready());
        // The weekly window is at 19%.
        harness.emit(cx, usage(12, None));
        let shown = |cx: &mut TestAppContext, label: &'static str| {
            harness.with_window(cx, move |window, _| {
                window.try_find_by_label(label).is_some()
            })
        };
        assert!(shown(cx, "Weekly 19%"));
        harness.emit(cx, usage(30, None));
        assert!(shown(cx, "Session 30%"));
    }

    #[gpui_kit::test]
    fn a_plan_with_only_a_weekly_window_shows_it(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.emit(cx, ready());
        harness.emit(
            cx,
            AgentEvent::Usage(Usage {
                weekly: Some(UsageWindow {
                    percent: 8,
                    resets_at: None,
                }),
                ..Usage::default()
            }),
        );
        let shown = |cx: &mut TestAppContext, label: &'static str| {
            harness.with_window(cx, move |window, _| {
                window.try_find_by_label(label).is_some()
            })
        };
        assert!(shown(cx, "Weekly 8%"));
        // Codex's free plan has one window, of 30 days.
        harness.emit(
            cx,
            AgentEvent::Usage(Usage {
                weekly: Some(UsageWindow {
                    percent: 8,
                    resets_at: None,
                }),
                long_window_days: Some(30),
                ..Usage::default()
            }),
        );
        assert!(shown(cx, "Monthly 8%"));
    }

    #[gpui_kit::test]
    fn the_header_shows_the_account_and_its_usage(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.emit(cx, ready());
        assert!(harness.commands().contains(&SessionCommand::RefreshUsage));
        harness.emit(cx, usage(42, None));
        harness.with_window(cx, |window, _| {
            assert!(window.find_by_label("dev@example.com").visible());
            assert!(window.find_by_label("Session 42%").visible());
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
    fn the_panel_tells_this_chat_s_part_of_the_session_from_the_rest(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.emit(cx, ready());
        harness.emit(cx, usage(30, None));
        harness.type_and_send(cx, "write the parser");
        harness.emit(cx, AgentEvent::PromptTokens(PromptTokens::default()));
        harness.emit(cx, usage(36, None));
        harness.emit(cx, AgentEvent::TurnEnded(TurnOutcome::Completed));
        harness.emit(cx, usage(40, None));
        // Another chat's work, after this one's turn.
        harness.emit(cx, usage(45, None));
        harness.with_window(cx, |window, cx| window.click("chat-usage", cx));
        harness.with_window(cx, |window, _| {
            let split = "About 10% in this chat, 35% elsewhere";
            assert!(window.find_by_label(split).visible());
            assert!(window.find("chat-usage-this-chat").visible());
        });
    }

    #[gpui_kit::test]
    fn usage_is_refreshed_while_the_chat_sits_idle(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.emit(cx, ready());
        harness.commands();
        cx.executor().advance_clock(Duration::from_mins(2));
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
