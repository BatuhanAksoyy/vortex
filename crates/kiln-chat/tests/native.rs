//! Commands Kiln answers itself: they never reach the agent as a message,
//! and work while a turn is running.

// Inside a `cfg(test)` module so helpers may `expect`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod native {
    use gpui_kit::TestAppContext;
    use gpui_kit::test::TestWindowExt;
    use kiln_agent::{
        AgentEvent, ModelOption, PermissionMode, SessionCommand, SessionInfo, SlashCommand, Usage,
        UsageWindow,
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
        let command = |name: &str| SlashCommand {
            name: name.into(),
            ..SlashCommand::default()
        };
        AgentEvent::Ready(SessionInfo {
            models: vec![
                option("default", "claude-opus-5-5", "Default (recommended)"),
                option("sonnet", "claude-sonnet-5-5", "Sonnet 5.5"),
                option("haiku", "claude-haiku-5-5", "Haiku 5.5"),
            ],
            mode: PermissionMode::AskBeforeEdits,
            account: None,
            commands: vec![command("usage"), command("model"), command("compact")],
        })
    }

    /// A chat whose agent is busy with a turn.
    fn busy(cx: &mut TestAppContext) -> Harness {
        let harness = Harness::new(cx);
        harness.emit(cx, ready());
        harness.emit(
            cx,
            AgentEvent::Usage(Usage {
                session: Some(UsageWindow {
                    percent: 40,
                    resets_at: None,
                }),
                ..Usage::default()
            }),
        );
        harness.type_and_send(cx, "work on it");
        assert_eq!(harness.status(cx), Status::Running);
        harness.commands();
        harness
    }

    fn input(harness: &Harness, cx: &mut TestAppContext, text: &str) {
        harness.with_window(cx, |window, cx| window.input(text, cx));
    }

    fn press(harness: &Harness, cx: &mut TestAppContext, key: &str) {
        harness.with_window(cx, |window, cx| window.press(key, cx));
    }

    fn shown(harness: &Harness, cx: &mut TestAppContext, id: &'static str) -> bool {
        harness.with_window(cx, |window, _| window.try_find(id).is_some())
    }

    #[gpui_kit::test]
    fn usage_opens_its_panel_mid_turn_without_a_message(cx: &mut TestAppContext) {
        let harness = busy(cx);
        input(&harness, cx, "/usage");
        press(&harness, cx, "enter");
        assert_eq!(harness.commands(), [SessionCommand::RefreshUsage]);
        assert!(shown(&harness, cx, "chat-usage-panel"));
        assert_eq!(harness.status(cx), Status::Running);
        press(&harness, cx, "escape");
        assert!(!shown(&harness, cx, "chat-usage-panel"));
    }

    #[gpui_kit::test]
    fn an_alias_typed_past_the_menu_still_runs_natively(cx: &mut TestAppContext) {
        let harness = busy(cx);
        input(&harness, cx, "/cost");
        press(&harness, cx, "enter");
        assert_eq!(harness.commands(), [SessionCommand::RefreshUsage]);
    }

    #[gpui_kit::test]
    fn model_opens_a_picker_that_switches_on_enter(cx: &mut TestAppContext) {
        let harness = busy(cx);
        input(&harness, cx, "/model");
        press(&harness, cx, "enter");
        assert!(shown(&harness, cx, "chat-model-picker"));
        assert_eq!(harness.commands(), Vec::<SessionCommand>::new());
        press(&harness, cx, "down");
        press(&harness, cx, "enter");
        assert_eq!(
            harness.commands(),
            [SessionCommand::SetModel("sonnet".into())]
        );
        assert!(!shown(&harness, cx, "chat-model-picker"));
    }

    #[gpui_kit::test]
    fn typing_puts_the_picker_away(cx: &mut TestAppContext) {
        let harness = busy(cx);
        input(&harness, cx, "/model");
        press(&harness, cx, "enter");
        input(&harness, cx, "x");
        assert!(!shown(&harness, cx, "chat-model-picker"));
    }

    #[gpui_kit::test]
    fn a_named_model_switches_at_once(cx: &mut TestAppContext) {
        let harness = busy(cx);
        input(&harness, cx, "/model haiku");
        press(&harness, cx, "enter");
        assert_eq!(
            harness.commands(),
            [SessionCommand::SetModel("haiku".into())]
        );
    }

    #[gpui_kit::test]
    fn clear_starts_a_new_conversation_mid_turn(cx: &mut TestAppContext) {
        let harness = busy(cx);
        let spawned = harness.script.borrow().spawned;
        input(&harness, cx, "/clear");
        press(&harness, cx, "enter");
        assert_eq!(harness.script.borrow().spawned, spawned + 1);
        assert_eq!(harness.status(cx), Status::Idle);
        assert!(shown(&harness, cx, "chat-empty"));
        assert!(
            harness.commands().is_empty(),
            "nothing reaches the new agent"
        );
    }
}
