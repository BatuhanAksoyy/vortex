//! The composer's strip: model and mode menus, context meter, Shift+Tab.

// Inside a `cfg(test)` module so helpers may `expect`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod composer {
    use gpui_kit::TestAppContext;
    use gpui_kit::test::{TestQueryExt, TestWindowExt};
    use kiln_agent::{
        AgentEvent, ContextUsage, ModelOption, PermissionMode, SessionCommand, SessionInfo,
        TurnOutcome,
    };
    use kiln_chat::Status;

    use super::common::Harness;

    fn ready() -> AgentEvent {
        let option = |id: &str, resolved: &str, name: &str| ModelOption {
            id: id.into(),
            resolved: resolved.into(),
            name: name.into(),
            description: format!("{name} for tests"),
        };
        AgentEvent::Ready(SessionInfo {
            models: vec![
                option("default", "claude-opus-5-5", "Default (recommended)"),
                option("opus", "claude-opus-5-5", "Opus 5.5"),
                option("sonnet", "claude-sonnet-5-5", "Sonnet 5.5"),
            ],
            mode: PermissionMode::AskBeforeEdits,
            account: None,
        })
    }

    fn started() -> AgentEvent {
        AgentEvent::Started {
            session_id: "s".into(),
            model: "claude-opus-5-5".into(),
        }
    }

    #[gpui_kit::test]
    fn the_composer_shows_the_agents_model_and_context(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.emit(cx, ready());
        harness.emit(cx, started());
        assert_eq!(
            harness.commands(),
            [SessionCommand::RefreshContext, SessionCommand::RefreshUsage]
        );
        harness.emit(
            cx,
            AgentEvent::ContextUsage(ContextUsage {
                used_tokens: 15_338,
                max_tokens: 200_000,
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
                SessionCommand::RefreshUsage
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
        assert_eq!(harness.commands(), [SessionCommand::RefreshContext]);
        harness.with_window(cx, |window, _| {
            assert!(window.find_by_label("Sonnet 5.5").visible());
        });
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
    fn a_new_process_keeps_the_chosen_model_and_mode(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        let spec = |harness: &Harness| {
            let script = harness.script.borrow();
            let spec = script.last_spec.as_ref().expect("spawned");
            (spec.model.clone(), spec.mode)
        };
        assert_eq!(spec(&harness), (None, None));
        harness.emit(cx, ready());
        harness.with_window(cx, |window, cx| window.click("chat-model", cx));
        harness.with_window(cx, |window, cx| window.click("chat-model-sonnet", cx));
        harness.emit(cx, AgentEvent::ModeChanged(PermissionMode::Plan));
        harness.with_window(cx, |window, cx| window.click("chat-new", cx));
        assert_eq!(
            spec(&harness),
            (Some("sonnet".into()), Some(PermissionMode::Plan))
        );
    }
}
