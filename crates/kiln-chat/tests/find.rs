//! Ctrl+F (Cmd+F on macOS) finds text in the conversation: messages, the
//! agent's text and tool calls, tool output when asked, and goes through
//! the matches with Enter and Shift+Enter.

// Inside a `cfg(test)` module so helpers may `expect`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod find {
    use gpui_kit::TestAppContext;
    use gpui_kit::test::{TestQueryExt, TestWindowExt};
    use kiln_agent::{AgentEvent, BlockKey, SessionCommand, ToolCall, TurnOutcome};
    use kiln_chat::Status;
    use serde_json::json;

    use super::common::Harness;

    const FIND: &str = if cfg!(target_os = "macos") {
        "cmd-f"
    } else {
        "ctrl-f"
    };
    const SELECT_ALL: &str = if cfg!(target_os = "macos") {
        "cmd-a"
    } else {
        "ctrl-a"
    };

    fn press(harness: &Harness, cx: &mut TestAppContext, key: &str) {
        harness.with_window(cx, |window, cx| window.press(key, cx));
    }

    fn shown(harness: &Harness, cx: &mut TestAppContext, id: &'static str) -> bool {
        harness.with_window(cx, |window, _| window.try_find(id).is_some())
    }

    fn labelled(harness: &Harness, cx: &mut TestAppContext, label: &str) -> bool {
        harness.with_window(cx, |window, _| !window.find_all_by_label(label).is_empty())
    }

    fn search(harness: &Harness, cx: &mut TestAppContext, query: &str) {
        press(harness, cx, FIND);
        harness.with_window(cx, |window, cx| window.input(query, cx));
    }

    /// "Fix the build" sent, the agent reading Cargo.toml with a tool whose
    /// output mentions the build, and answering about the build.
    fn conversation(cx: &mut TestAppContext) -> Harness {
        let harness = Harness::new(cx);
        harness.type_and_send(cx, "Fix the build");
        harness.emit(
            cx,
            AgentEvent::ToolCall(ToolCall {
                id: "tool-1".into(),
                parent: None,
                name: "Read".into(),
                input: json!({ "file_path": "/tmp/Cargo.toml" }),
            }),
        );
        harness.emit(
            cx,
            AgentEvent::ToolResult {
                id: "tool-1".into(),
                output: "[profile.release] # build settings".into(),
                pictures: Vec::new(),
                is_error: false,
            },
        );
        harness.emit(
            cx,
            AgentEvent::TextDelta {
                block: BlockKey {
                    message_id: "m1".into(),
                    index: 0,
                },
                text: "The **build** fails in Cargo.toml.".into(),
            },
        );
        harness.emit(cx, AgentEvent::TurnEnded(TurnOutcome::Completed));
        harness
    }

    #[gpui_kit::test]
    fn finds_messages_and_goes_through_the_matches(cx: &mut TestAppContext) {
        let harness = conversation(cx);
        search(&harness, cx, "BUILD");
        assert!(shown(&harness, cx, "chat-find"));
        assert!(
            labelled(&harness, cx, "1 of 2"),
            "the message and the reply"
        );
        assert!(
            !labelled(&harness, cx, "Search match"),
            "text shows its matches, not its row"
        );
        press(&harness, cx, "enter");
        assert!(labelled(&harness, cx, "2 of 2"));
        press(&harness, cx, "enter");
        assert!(labelled(&harness, cx, "1 of 2"), "round the end");
        press(&harness, cx, "shift-enter");
        assert!(labelled(&harness, cx, "2 of 2"));
    }

    #[gpui_kit::test]
    fn tool_calls_match_and_tool_output_only_when_asked(cx: &mut TestAppContext) {
        let harness = conversation(cx);
        search(&harness, cx, "cargo.toml");
        assert!(labelled(&harness, cx, "1 of 2"), "the call and the reply");
        assert!(
            labelled(&harness, cx, "Search match"),
            "the call's row is marked"
        );
        press(&harness, cx, SELECT_ALL);
        harness.with_window(cx, |window, cx| window.input("profile.release", cx));
        assert!(labelled(&harness, cx, "No results"));
        harness.with_window(cx, |window, cx| window.click("chat-find-output", cx));
        assert!(labelled(&harness, cx, "1 of 1"), "found in the output");
    }

    #[gpui_kit::test]
    fn escape_closes_the_search(cx: &mut TestAppContext) {
        let harness = conversation(cx);
        search(&harness, cx, "build");
        press(&harness, cx, "escape");
        assert!(!shown(&harness, cx, "chat-find"));
        assert!(!labelled(&harness, cx, "Search match"));
        search(&harness, cx, "nothing like this");
        assert!(labelled(&harness, cx, "No results"));
    }

    #[gpui_kit::test]
    fn enter_in_the_search_sends_nothing(cx: &mut TestAppContext) {
        let harness = conversation(cx);
        harness.commands();
        let view = harness.view.clone();
        harness.with_window(cx, |window, cx| {
            view.update(cx, |view, cx| view.open_find(window, cx));
        });
        harness.with_window(cx, |window, cx| window.input("build", cx));
        press(&harness, cx, "enter");
        let sent = harness
            .commands()
            .into_iter()
            .any(|command| matches!(command, SessionCommand::Send(_)));
        assert!(!sent);
        assert_eq!(harness.status(cx), Status::Idle);
        assert!(labelled(&harness, cx, "2 of 2"));
    }
}
