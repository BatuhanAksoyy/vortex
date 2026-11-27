//! The composer's strip: model, mode and effort menus, context meter and
//! prompt cache, Shift+Tab.

// Inside a `cfg(test)` module so helpers may `expect`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod composer {
    use gpui_kit::TestAppContext;
    use gpui_kit::test::{TestQueryExt, TestWindowExt};
    use std::time::Duration;

    use vortex_agent::{
        AgentEvent, ContextUsage, Effort, ModelOption, PermissionMode, PromptTokens,
        SessionCommand, SessionInfo, SpentTokens, TurnOutcome,
    };
    use vortex_chat::Status;

    use super::common::Harness;

    fn ready() -> AgentEvent {
        let option = |id: &str, resolved: &str, name: &str| ModelOption {
            id: id.into(),
            resolved: resolved.into(),
            name: name.into(),
            description: format!("{name} for tests"),
            efforts: vec![Effort::Low, Effort::High, Effort::Max],
        };
        AgentEvent::Ready(SessionInfo {
            models: vec![
                option("default", "claude-opus-5-5", "Default (recommended)"),
                option("opus", "claude-opus-5-5", "Opus 5.5"),
                option("sonnet", "claude-sonnet-5-5", "Sonnet 5.5"),
            ],
            mode: PermissionMode::AskBeforeEdits,
            account: None,
            commands: Vec::new(),
        })
    }

    fn started() -> AgentEvent {
        AgentEvent::Started {
            session_id: "s".into(),
            model: "claude-opus-5-5".into(),
            cli_version: None,
        }
    }

    #[gpui_kit::test]
    fn the_composer_shows_the_agents_model_and_context(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.emit(cx, ready());
        harness.emit(cx, started());
        assert_eq!(
            harness.commands(),
            [
                SessionCommand::RefreshContext,
                SessionCommand::RefreshUsage,
                SessionCommand::RefreshEffort,
                SessionCommand::RefreshModels
            ]
        );
        harness.emit(
            cx,
            AgentEvent::ContextUsage(ContextUsage {
                used_tokens: 15_338,
                max_tokens: 200_000,
                ..ContextUsage::default()
            }),
        );
        harness.with_window(cx, |window, _| {
            assert!(window.find_by_label("Opus 5.5").visible());
            assert!(window.find("chat-context").visible());
        });
        harness.type_and_send(cx, "hi");
        assert_eq!(harness.status(cx), Status::Running);
        harness.emit(cx, AgentEvent::TurnEnded(TurnOutcome::Completed));
        assert_eq!(
            harness.commands(),
            [
                SessionCommand::Send("hi".into()),
                SessionCommand::RefreshContext,
                SessionCommand::RefreshUsage,
                SessionCommand::RefreshEffort,
                SessionCommand::RefreshModels
            ]
        );
    }

    #[gpui_kit::test]
    fn picking_a_model_asks_the_agent_to_switch(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.emit(cx, ready());
        harness.emit(cx, started());
        harness.commands();
        harness.with_window(cx, |window, cx| window.click("chat-model", cx));
        harness.with_window(cx, |window, _| {
            let button = window.find("chat-model").bounds();
            let option = window.find("chat-model-sonnet").bounds();
            assert!(
                option.bottom() <= button.top(),
                "the menu opens above the composer"
            );
        });
        harness.with_window(cx, |window, cx| window.click("chat-model-sonnet", cx));
        assert_eq!(
            harness.commands(),
            [SessionCommand::SetModel("sonnet".into())]
        );
        harness.emit(cx, AgentEvent::ModelChanged("claude-sonnet-5-5".into()));
        assert_eq!(
            harness.commands(),
            [
                SessionCommand::RefreshContext,
                SessionCommand::RefreshEffort
            ]
        );
        harness.with_window(cx, |window, _| {
            assert!(window.find_by_label("Sonnet 5.5").visible());
        });
    }

    #[gpui_kit::test]
    fn bypassing_permissions_is_confirmed_first(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.emit(cx, ready());
        harness.commands();
        let ask = |cx: &mut TestAppContext| {
            harness.with_window(cx, |window, cx| window.click("chat-mode", cx));
            harness.with_window(cx, |window, cx| {
                window.click("chat-mode-BypassPermissions", cx);
            });
        };
        ask(cx);
        harness.with_window(cx, |window, cx| window.click("cancel", cx));
        assert_eq!(harness.commands(), []);
        ask(cx);
        assert_eq!(harness.commands(), []);
        harness.with_window(cx, |window, cx| window.click("ok", cx));
        assert_eq!(
            harness.commands(),
            [SessionCommand::SetMode(PermissionMode::BypassPermissions)]
        );
        let mode = PermissionMode::BypassPermissions;
        harness.emit(cx, AgentEvent::ModeChanged(mode));
        cx.update(|cx| {
            let chat = harness.view.read(cx);
            assert_eq!((chat.mode(), chat.known_mode()), (mode, None));
        });
    }

    #[gpui_kit::test]
    fn tab_takes_the_message_the_agent_predicts(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.emit(cx, ready());
        harness.emit(cx, started());
        harness.commands();
        harness.emit(cx, AgentEvent::PromptSuggestion("run the tests".into()));
        harness.with_window(cx, |window, cx| window.click("chat-composer", cx));
        harness.with_window(cx, |window, cx| window.press("tab", cx));
        harness.with_window(cx, |window, cx| window.press("enter", cx));
        let sent = harness.commands();
        assert!(
            matches!(&sent[..], [SessionCommand::Send(message), ..] if message.text == "run the tests"),
            "{sent:?}"
        );
    }

    #[gpui_kit::test]
    fn typing_drops_the_predicted_message(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.emit(cx, ready());
        harness.emit(cx, started());
        harness.emit(cx, AgentEvent::PromptSuggestion("run the tests".into()));
        harness.commands();
        harness.type_and_send(cx, "no");
        let sent = harness.commands();
        assert!(
            matches!(&sent[..], [SessionCommand::Send(message), ..] if message.text == "no"),
            "{sent:?}"
        );
        harness.with_window(cx, |window, cx| window.press("tab", cx));
        cx.update(|cx| assert_eq!(harness.view.read(cx).status(), Status::Running));
    }

    #[gpui_kit::test]
    fn the_mode_changes_only_when_the_agent_confirms(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.emit(cx, ready());
        harness.commands();
        harness.with_window(cx, |window, cx| window.click("chat-mode", cx));
        harness.with_window(cx, |window, cx| window.click("chat-mode-Auto", cx));
        assert_eq!(
            harness.commands(),
            [SessionCommand::SetMode(PermissionMode::Auto)]
        );
        let mode = |cx: &mut TestAppContext| cx.update(|cx| harness.view.read(cx).mode());
        assert_eq!(mode(cx), PermissionMode::AskBeforeEdits);
        harness.emit(cx, AgentEvent::ModeChanged(PermissionMode::Auto));
        assert_eq!(mode(cx), PermissionMode::Auto);
    }

    #[gpui_kit::test]
    fn the_effort_menu_offers_the_models_efforts(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.emit(cx, ready());
        harness.emit(cx, started());
        assert!(
            !harness.with_window(cx, |window, _| window.try_find("chat-effort").is_some()),
            "hidden until the agent names the effort"
        );
        harness.emit(cx, AgentEvent::EffortChanged(Some(Effort::High)));
        harness.commands();
        harness.with_window(cx, |window, cx| window.click("chat-effort", cx));
        harness.with_window(cx, |window, _| {
            assert!(window.find("chat-effort-Max").visible());
            assert!(window.try_find("chat-effort-Medium").is_none());
        });
        harness.with_window(cx, |window, cx| window.click("chat-effort-Max", cx));
        assert_eq!(harness.commands(), [SessionCommand::SetEffort(Effort::Max)]);
        harness.emit(cx, AgentEvent::EffortChanged(Some(Effort::Max)));
        harness.with_window(cx, |window, _| {
            assert!(window.find_by_label("Max").visible());
        });
        harness.emit(cx, AgentEvent::EffortChanged(None));
        assert!(
            !harness.with_window(cx, |window, _| window.try_find("chat-effort").is_some()),
            "a model that takes no effort hides it"
        );
    }

    #[gpui_kit::test]
    fn the_strip_tells_what_the_conversation_spent(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.emit(cx, ready());
        harness.emit(cx, started());
        let spent = SpentTokens {
            input: 2,
            cached: 10_000,
            written: 4_998,
            output: 300,
        };
        harness.emit(cx, AgentEvent::SpentTokens(spent));
        harness.with_window(cx, |window, cx| window.click("chat-tokens", cx));
        harness.with_window(cx, |window, _| {
            let panel = window.find("chat-tokens-panel");
            assert_eq!(panel.label(), Some("15.3k tokens spent"));
        });
        harness.emit(cx, AgentEvent::ConversationReset);
        assert!(!harness.with_window(cx, |window, _| window.try_find("chat-tokens").is_some()));
    }

    #[gpui_kit::test]
    fn the_cache_button_tells_how_the_prompt_was_cached(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.emit(cx, ready());
        harness.emit(cx, started());
        harness.with_window(cx, |window, cx| window.click("chat-cache", cx));
        harness.with_window(cx, |window, _| {
            assert!(window.find("chat-cache-empty").visible());
        });
        harness.with_window(cx, |window, cx| window.click("chat-cache", cx));
        harness.emit(
            cx,
            AgentEvent::PromptTokens(PromptTokens {
                cached: 38_520,
                written: 6_662,
                uncached: 2,
                kept_for: Some(Duration::from_hours(1)),
            }),
        );
        harness.with_window(cx, |window, cx| window.click("chat-cache", cx));
        harness.with_window(cx, |window, _| {
            assert!(window.find("chat-cache-panel").visible());
            let status = window.find("chat-cache-status");
            assert!(
                status
                    .label()
                    .is_some_and(|label| label.starts_with("Warm until")),
                "{:?}",
                status.label()
            );
        });
    }

    #[gpui_kit::test]
    fn a_reopened_chat_knows_its_cache_expired(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.emit(
            cx,
            AgentEvent::PastPromptTokens {
                tokens: PromptTokens {
                    cached: 20_000,
                    written: 100,
                    uncached: 2,
                    kept_for: Some(Duration::from_hours(1)),
                },
                at: std::time::SystemTime::now() - Duration::from_hours(2),
            },
        );
        harness.with_window(cx, |window, _| {
            assert!(window.find_by_label("Cache expired").visible());
        });
    }

    #[gpui_kit::test]
    fn shift_tab_in_the_composer_cycles_the_mode(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.emit(cx, ready());
        harness.commands();
        harness.with_window(cx, |window, cx| window.click("chat-composer", cx));
        harness.with_window(cx, |window, cx| window.press("shift-tab", cx));
        assert_eq!(
            harness.commands(),
            [SessionCommand::SetMode(PermissionMode::EditAutomatically)]
        );
    }

    #[gpui_kit::test]
    fn a_new_process_keeps_the_chosen_model_mode_and_effort(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        let spec = |harness: &Harness| {
            let script = harness.script.borrow();
            let spec = script.last_spec.as_ref().expect("spawned");
            (spec.model.clone(), spec.mode, spec.effort)
        };
        assert_eq!(spec(&harness), (None, None, None));
        harness.emit(cx, ready());
        harness.with_window(cx, |window, cx| window.click("chat-model", cx));
        harness.with_window(cx, |window, cx| window.click("chat-model-sonnet", cx));
        harness.emit(cx, AgentEvent::ModeChanged(PermissionMode::Plan));
        harness.emit(cx, AgentEvent::EffortChanged(Some(Effort::Low)));
        harness.with_window(cx, |window, cx| window.click("chat-new", cx));
        assert_eq!(
            spec(&harness),
            (
                Some("sonnet".into()),
                Some(PermissionMode::Plan),
                Some(Effort::Low)
            )
        );
    }
}
