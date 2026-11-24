//! A subagent's card: its progress while it runs, its steps when opened,
//! and the turn its report starts after the one that started it ended.

// Inside a `cfg(test)` module so helpers may `expect`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod subagents {
    use std::time::Duration;

    use gpui_kit::TestAppContext;
    use gpui_kit::test::{TestQueryExt, TestWindowExt};
    use kiln_agent::{
        AgentEvent, PromptTokens, SubagentChange, SubagentEnd, SubagentEvent, SubagentUsage,
        ToolCall, TurnOutcome,
    };
    use kiln_chat::Status;
    use serde_json::json;

    use super::common::Harness;

    fn call(id: &str, parent: Option<&str>, name: &str) -> AgentEvent {
        AgentEvent::ToolCall(ToolCall {
            id: id.into(),
            parent: parent.map(str::to_owned),
            name: name.into(),
            input: json!({ "file_path": "note.txt" }),
        })
    }

    fn change(change: SubagentChange) -> AgentEvent {
        AgentEvent::Subagent(SubagentEvent {
            call: "agent".into(),
            change,
        })
    }

    fn label(harness: &Harness, cx: &mut TestAppContext, id: &'static str) -> String {
        harness.with_window(cx, |window, _| {
            window.find(id).label().map_or_default(str::to_owned)
        })
    }

    /// A turn whose subagent has started and made one call.
    fn started(cx: &mut TestAppContext) -> Harness {
        let harness = Harness::new(cx);
        harness.type_and_send(cx, "read the note through an agent");
        assert_eq!(
            harness.commands(),
            [kiln_agent::SessionCommand::Send(
                "read the note through an agent".into()
            )]
        );
        harness.emit(cx, call("agent", None, "Agent"));
        harness.emit(
            cx,
            change(SubagentChange::Started {
                description: "Read the note".into(),
                kind: Some("general-purpose".into()),
            }),
        );
        harness.emit(cx, call("read", Some("agent"), "Read"));
        harness
    }

    #[gpui_kit::test]
    fn a_subagent_shows_as_a_card_with_its_steps_inside(cx: &mut TestAppContext) {
        let harness = started(cx);
        let usage = SubagentUsage {
            tokens: 14_399,
            tool_uses: 1,
            duration: Duration::from_millis(1_180),
        };
        harness.emit(
            cx,
            change(SubagentChange::Progress {
                activity: "Reading note.txt".into(),
                usage,
            }),
        );
        harness.with_window(cx, |window, _| {
            assert!(window.find_by_label("Read the note").visible());
        });
        assert_eq!(
            label(&harness, cx, "subagent-status-2"),
            "Reading note.txt · 1 tool · 14.3k tokens · 1.2s"
        );
        assert!(
            harness.with_window(cx, |window, _| window.try_find_by_label("Read").is_none()),
            "the subagent's call stays in its card"
        );

        harness.with_window(cx, |window, cx| window.click("subagent-2", cx));
        harness.with_window(cx, |window, _| {
            assert!(window.find_by_label("Read").visible());
        });

        harness.emit(cx, AgentEvent::TurnEnded(TurnOutcome::Completed));
        assert_eq!(harness.status(cx), Status::Idle);
        harness.emit(
            cx,
            change(SubagentChange::Finished {
                end: SubagentEnd::Completed,
                summary: "lantern".into(),
                usage: Some(usage),
            }),
        );
        assert!(label(&harness, cx, "subagent-status-2").starts_with("Done · 1 tool"));
        harness.emit(cx, AgentEvent::PromptTokens(PromptTokens::default()));
        assert_eq!(
            harness.status(cx),
            Status::Running,
            "the report starts a turn of its own"
        );
    }

    #[gpui_kit::test]
    fn the_agents_dialog_shows_each_subagents_own_chat(cx: &mut TestAppContext) {
        let harness = started(cx);
        harness.emit(cx, change(SubagentChange::Model("haiku".into())));
        harness.emit(
            cx,
            change(SubagentChange::Request(PromptTokens {
                cached: 14_374,
                written: 2_948,
                uncached: 2,
                kept_for: None,
            })),
        );
        harness.with_window(cx, |window, _| {
            assert_eq!(window.find("chat-agents").label(), Some("1 agent running"));
        });
        harness.emit(
            cx,
            change(SubagentChange::Finished {
                end: SubagentEnd::Completed,
                summary: "lantern".into(),
                usage: None,
            }),
        );
        harness.with_window(cx, |window, cx| {
            assert_eq!(window.find("chat-agents").label(), Some("1 agent"));
            window.click("chat-agents", cx);
        });
        harness.with_window(cx, |window, cx| {
            assert!(window.find("chat-agents-dialog").visible());
            window.press("escape", cx);
        });
        assert_eq!(
            harness.status(cx),
            Status::Running,
            "Escape closes the view, not the turn"
        );
        harness.with_window(cx, |window, cx| {
            assert!(window.try_find("chat-agents-dialog").is_none());
            window.click("chat-agents", cx);
        });
        harness.with_window(cx, |window, cx| window.click("chat-agent-2", cx));
        assert_eq!(label(&harness, cx, "chat-agent-detail"), "Read the note");
        assert_eq!(label(&harness, cx, "chat-agent-model"), "haiku");
        assert!(label(&harness, cx, "chat-agent-context").starts_with("17.3k tokens"));
        harness.with_window(cx, |window, cx| {
            assert!(
                window.find_by_label("Read").visible(),
                "its tool calls show"
            );
            window.click("chat-agent-reveal", cx);
        });
        harness.with_window(cx, |window, _| {
            assert!(window.try_find("chat-agents-dialog").is_none());
            assert!(
                window.find_by_label("Read").visible(),
                "the card opened on its steps"
            );
        });
    }
}
