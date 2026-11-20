//! The `/` command menu: matching, keyboard choice, completion, and the
//! conversation starting over when the agent clears it.

// Inside a `cfg(test)` module so helpers may `expect`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod slash {
    use gpui_kit::TestAppContext;
    use gpui_kit::test::TestWindowExt;
    use kiln_agent::{AgentEvent, PermissionMode, SessionCommand, SessionInfo, SlashCommand};

    use super::common::Harness;

    fn command(name: &str, hint: &str, aliases: &[&str]) -> SlashCommand {
        SlashCommand {
            name: name.into(),
            description: format!("{name} for tests"),
            argument_hint: hint.into(),
            aliases: aliases.iter().map(|alias| (*alias).into()).collect(),
        }
    }

    fn ready(cx: &mut TestAppContext) -> Harness {
        let harness = Harness::new(cx);
        harness.emit(
            cx,
            AgentEvent::Ready(SessionInfo {
                models: Vec::new(),
                mode: PermissionMode::AskBeforeEdits,
                account: None,
                commands: vec![
                    command("compact", "<optional instructions>", &[]),
                    command("context", "", &[]),
                    command("clear", "[name]", &["reset", "new"]),
                ],
            }),
        );
        harness.commands();
        harness
    }

    fn type_text(harness: &Harness, cx: &mut TestAppContext, text: &str) {
        harness.with_window(cx, |window, cx| window.click("chat-composer", cx));
        type_more(harness, cx, text);
    }

    /// Types at the cursor, where the last edit left it.
    fn type_more(harness: &Harness, cx: &mut TestAppContext, text: &str) {
        harness.with_window(cx, |window, cx| window.input(text, cx));
    }

    fn press(harness: &Harness, cx: &mut TestAppContext, key: &str) {
        harness.with_window(cx, |window, cx| window.press(key, cx));
    }

    fn menu_open(harness: &Harness, cx: &mut TestAppContext) -> bool {
        harness.with_window(cx, |window, _| window.try_find("chat-slash-menu").is_some())
    }

    #[gpui_kit::test]
    fn the_menu_lists_matching_commands_while_the_name_is_typed(cx: &mut TestAppContext) {
        let harness = ready(cx);
        type_text(&harness, cx, "/co");
        harness.with_window(cx, |window, _| {
            assert!(window.find("chat-slash-compact").visible());
            assert!(window.find("chat-slash-context").visible());
            assert!(window.try_find("chat-slash-clear").is_none());
            let menu = window.find("chat-slash-menu").bounds();
            let composer = window.find("chat-composer").bounds();
            assert!(menu.bottom() <= composer.top(), "the menu opens above");
        });
        type_more(&harness, cx, " now");
        assert!(!menu_open(&harness, cx), "arguments close the menu");
    }

    #[gpui_kit::test]
    fn enter_runs_the_chosen_command_once(cx: &mut TestAppContext) {
        let harness = ready(cx);
        type_text(&harness, cx, "/co");
        press(&harness, cx, "down");
        press(&harness, cx, "enter");
        assert_eq!(
            harness.commands(),
            [SessionCommand::Send("/context".into())]
        );
        assert_eq!(harness.status(cx), kiln_chat::Status::Running);
        assert!(!menu_open(&harness, cx));
    }

    #[gpui_kit::test]
    fn tab_completes_the_name_and_leaves_room_for_arguments(cx: &mut TestAppContext) {
        let harness = ready(cx);
        type_text(&harness, cx, "/comp");
        press(&harness, cx, "tab");
        assert!(harness.commands().is_empty(), "tab does not send");
        assert!(!menu_open(&harness, cx));
        type_more(&harness, cx, "keep the plan");
        press(&harness, cx, "enter");
        assert_eq!(
            harness.commands(),
            [SessionCommand::Send("/compact keep the plan".into())]
        );
    }

    #[gpui_kit::test]
    fn an_alias_finds_its_command(cx: &mut TestAppContext) {
        let harness = ready(cx);
        type_text(&harness, cx, "/new");
        harness.with_window(cx, |window, _| {
            assert!(window.find("chat-slash-clear").visible());
        });
    }

    #[gpui_kit::test]
    fn escape_closes_the_menu_until_the_name_changes(cx: &mut TestAppContext) {
        let harness = ready(cx);
        type_text(&harness, cx, "/co");
        press(&harness, cx, "escape");
        assert!(!menu_open(&harness, cx));
        press(&harness, cx, "enter");
        assert_eq!(
            harness.commands(),
            [SessionCommand::Send("/co".into())],
            "a closed menu leaves Enter to send"
        );
        harness.emit(
            cx,
            AgentEvent::TurnEnded(kiln_agent::TurnOutcome::Completed),
        );
        harness.commands();
        type_text(&harness, cx, "/c");
        press(&harness, cx, "escape");
        type_more(&harness, cx, "o");
        assert!(menu_open(&harness, cx));
    }

    #[gpui_kit::test]
    fn sending_with_the_button_closes_the_menu(cx: &mut TestAppContext) {
        let harness = ready(cx);
        type_text(&harness, cx, "/co");
        harness.with_window(cx, |window, cx| window.click("chat-send", cx));
        assert_eq!(harness.commands(), [SessionCommand::Send("/co".into())]);
        assert!(!menu_open(&harness, cx));
    }

    #[gpui_kit::test]
    fn clicking_a_command_completes_it(cx: &mut TestAppContext) {
        let harness = ready(cx);
        type_text(&harness, cx, "/");
        harness.with_window(cx, |window, cx| window.click("chat-slash-compact", cx));
        assert!(!menu_open(&harness, cx));
        press(&harness, cx, "enter");
        assert_eq!(
            harness.commands(),
            [SessionCommand::Send("/compact".into())]
        );
    }

    /// Kiln answers `/clear` itself, but the agent may still start over on
    /// its own, e.g. from a skill.
    #[gpui_kit::test]
    fn a_reset_by_the_agent_empties_the_transcript(cx: &mut TestAppContext) {
        let harness = ready(cx);
        harness.type_and_send(cx, "hello");
        let empty = |cx: &mut TestAppContext| {
            harness.with_window(cx, |window, _| window.try_find("chat-empty").is_some())
        };
        assert!(!empty(cx));
        harness.emit(cx, AgentEvent::ConversationReset);
        assert!(empty(cx));
    }
}
