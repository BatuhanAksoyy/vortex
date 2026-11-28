//! An added account's plan usage: asked apart from the chat's session, and
//! left alone for a while once Claude refuses to say it.

// Inside a `cfg(test)` module so helpers may `expect`, like unit tests.
#[cfg(test)]
mod account_support;
#[cfg(test)]
mod common;

#[cfg(test)]
mod account_usage {
    use gpui_kit::TestAppContext;
    use gpui_kit::test::{TestQueryExt, TestWindowExt};
    use vortex_agent::{AgentEvent, SessionCommand, SessionInfo, TurnOutcome, Usage, UsageWindow};

    use vortex_chat::Status;

    use super::account_support::{add, click, ready, shown};

    #[gpui_kit::test]
    fn an_added_account_is_asked_for_usage_apart_and_at_once(cx: &mut TestAppContext) {
        let harness = ready(cx);
        let session = Some(UsageWindow {
            percent: 12,
            resets_at: None,
        });
        harness.script.borrow_mut().usage = Some(Usage {
            session,
            ..Usage::default()
        });
        click(&harness, cx, "chat-account");
        add(&harness, cx, "second@example.com");
        harness.emit(cx, AgentEvent::Ready(SessionInfo::default()));
        // The session would answer with the last-signed-in account's numbers.
        let asked = harness.commands();
        assert!(!asked.contains(&SessionCommand::RefreshUsage), "{asked:?}");
        let asks = harness.script.borrow().usage_asks.clone();
        assert_eq!(asks.len(), 1);
        let config = asks[0].iter().any(|(var, _)| var == "CLAUDE_CONFIG_DIR");
        assert!(config, "a configuration of the account's own");
        assert!(harness.with_window(cx, |window, _| {
            window.try_find_by_label("Session 12%").is_some()
        }));

        harness.emit(cx, AgentEvent::TurnEnded(TurnOutcome::Completed));
        assert_eq!(
            harness.script.borrow().usage_asks.len(),
            1,
            "it was answered just now"
        );
    }

    #[gpui_kit::test]
    fn the_panel_shows_another_account_s_usage_before_moving_to_it(cx: &mut TestAppContext) {
        let harness = ready(cx);
        click(&harness, cx, "chat-account");
        add(&harness, cx, "first@example.com");
        let second = add(&harness, cx, "second@example.com");
        harness.emit(cx, AgentEvent::Ready(SessionInfo::default()));
        let asked = harness.script.borrow().usage_asks.len();
        let lane = format!("chat-account-usage-{second}");
        let before = lane.clone();
        let label = harness.with_window(cx, move |window, _| {
            window.find(before).label().map(str::to_owned)
        });
        assert_eq!(label.as_deref(), Some("No usage reported yet"));
        harness.script.borrow_mut().usage = Some(Usage {
            session: Some(UsageWindow {
                percent: 64,
                resets_at: None,
            }),
            weekly: Some(UsageWindow {
                percent: 30,
                resets_at: None,
            }),
            ..Usage::default()
        });
        // Closed and opened again.
        click(&harness, cx, "chat-account");
        click(&harness, cx, "chat-account");
        assert_eq!(
            harness.script.borrow().usage_asks.len(),
            asked + 1,
            "the other account, asked apart"
        );
        harness.with_window(cx, move |window, _| {
            let lane = window.find(lane);
            assert!(lane.visible(), "under its name");
            assert_eq!(
                lane.label(),
                Some("Session 64% used · Weekly 30% used"),
                "both windows in one bar"
            );
        });
    }

    #[gpui_kit::test]
    fn a_refused_ask_says_so_and_is_not_repeated(cx: &mut TestAppContext) {
        let harness = ready(cx);
        click(&harness, cx, "chat-account");
        add(&harness, cx, "second@example.com");
        harness.emit(cx, AgentEvent::Ready(SessionInfo::default()));
        assert_eq!(harness.script.borrow().usage_asks.len(), 1);
        click(&harness, cx, "chat-usage");
        assert!(harness.with_window(cx, |window, _| window.find("chat-usage-refused").visible()));
        harness.emit(cx, AgentEvent::TurnEnded(TurnOutcome::Completed));
        cx.update(|cx| {
            harness
                .view
                .update(cx, vortex_chat::ChatView::refresh_usage);
        });
        let asks = harness.script.borrow().usage_asks.len();
        assert_eq!(asks, 1, "it waits before asking again");

        // A turn's own report still brings the numbers in.
        harness.type_and_send(cx, "hello");
        assert_eq!(harness.status(cx), Status::Running);
        let session = Some(UsageWindow {
            percent: 7,
            resets_at: None,
        });
        harness.emit(
            cx,
            AgentEvent::Usage(Usage {
                session,
                ..Usage::default()
            }),
        );
        assert!(shown(&harness, cx, "Session 7%"));
        let refused = harness.with_window(cx, |window, _| window.try_find("chat-usage-refused"));
        assert!(refused.is_none(), "the account answered");
    }
}
