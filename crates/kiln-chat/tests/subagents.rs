//! A subagent's card: its progress while it runs, its steps when opened,
//! and the turn its report starts after the one that started it ended.

// Inside a `cfg(test)` module so helpers may `expect`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod subagents {
    use std::time::Duration;

    use gpui_kit::test::{TestQueryExt, TestWindowExt};
    use gpui_kit::{TestAppContext, point, px};
    use kiln_agent::{
        AgentEvent, PromptTokens, SessionCommand, SubagentChange, SubagentEnd, SubagentEvent,
        SubagentUsage, ToolCall, TurnOutcome,
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
        change_of("agent", change)
    }

    fn change_of(call: &str, change: SubagentChange) -> AgentEvent {
        let call = call.into();
        AgentEvent::Subagent(SubagentEvent { call, change })
    }

    fn label(harness: &Harness, cx: &mut TestAppContext, id: &'static str) -> String {
        harness.with_window(cx, |window, _| {
            window.find(id).label().map_or_default(str::to_owned)
        })
    }

    #[gpui_kit::test]
    fn the_button_shows_before_any_subagent_starts(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        assert_eq!(label(&harness, cx, "chat-agents"), "Agents 0");
    }

    /// A turn whose subagent has started and made one call.
    fn started(cx: &mut TestAppContext) -> Harness {
        let harness = Harness::new(cx);
        harness.type_and_send(cx, "read the note through an agent");
        assert_eq!(
            harness.commands(),
            [SessionCommand::Send(
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
            assert_eq!(window.find("chat-agents").label(), Some("Agents 1"));
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
            assert_eq!(window.find("chat-agents").label(), Some("Agents 0"));
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

    /// The subagent of [`started`], card 2, has started one of its own,
    /// card 3, which made a call.
    fn nested(cx: &mut TestAppContext) -> Harness {
        let harness = started(cx);
        harness.emit(cx, call("inner", Some("agent"), "Agent"));
        let started = SubagentChange::Started {
            description: "Count the words".into(),
            kind: None,
        };
        harness.emit(cx, change_of("inner", started));
        harness.emit(cx, call("grep", Some("inner"), "Grep"));
        harness
    }

    #[gpui_kit::test]
    fn a_subagents_own_subagent_is_a_card_inside_its_card(cx: &mut TestAppContext) {
        let harness = nested(cx);
        assert_eq!(
            label(&harness, cx, "subagent-status-2"),
            "Starting… · 1 subagent, 1 running"
        );
        let has = |cx: &mut TestAppContext, label: &'static str| {
            harness.with_window(cx, |window, _| window.try_find_by_label(label).is_some())
        };
        assert!(!has(cx, "Count the words"), "inside the closed card");
        harness.with_window(cx, |window, cx| window.click("subagent-2", cx));
        assert!(has(cx, "Count the words") && !has(cx, "Grep"));
        harness.with_window(cx, |window, cx| window.click("subagent-3", cx));
        assert!(has(cx, "Grep") && has(cx, "Read"), "only its own opened");
        harness.with_window(cx, |window, cx| window.click("subagent-open-3", cx));
        assert_eq!(label(&harness, cx, "chat-agent-detail"), "Count the words");
    }

    #[gpui_kit::test]
    fn the_agents_dialog_lists_subagents_as_a_tree(cx: &mut TestAppContext) {
        let harness = nested(cx);
        assert_eq!(label(&harness, cx, "chat-agents"), "Agents 2");
        harness.with_window(cx, |window, cx| window.click("chat-agents", cx));
        assert_eq!(label(&harness, cx, "chat-agent-detail"), "Read the note");
        harness.with_window(cx, |window, cx| window.click("chat-agent-inner-3", cx));
        assert_eq!(label(&harness, cx, "chat-agent-detail"), "Count the words");
        harness.with_window(cx, |window, cx| window.click("chat-agent-above-2", cx));
        assert_eq!(label(&harness, cx, "chat-agent-detail"), "Read the note");
        harness.with_window(cx, |window, cx| window.press("down", cx));
        assert_eq!(label(&harness, cx, "chat-agent-detail"), "Count the words");
        harness.with_window(cx, |window, cx| window.click("chat-agent-reveal", cx));
        harness.with_window(cx, |window, _| {
            assert!(window.try_find("chat-agents-dialog").is_none());
            assert!(window.find_by_label("Grep").visible(), "both cards opened");
        });
    }

    #[gpui_kit::test]
    fn the_agent_map_opens_from_the_list_and_leads_to_an_agent(cx: &mut TestAppContext) {
        let harness = nested(cx);
        harness.with_window(cx, |window, cx| window.click("chat-agents", cx));
        harness.with_window(cx, |window, cx| window.click("chat-agents-map-show", cx));
        harness.with_window(cx, |window, _| {
            let outer = window.find("chat-agents-map-2").bounds();
            let inner = window.find("chat-agents-map-3").bounds();
            assert!(inner.left() > outer.right(), "to the right of its parent");
            assert_eq!(inner.top(), outer.top(), "the first on its line");
            assert!(window.try_find("chat-agents-list").is_none());
        });
        let click = |cx: &mut TestAppContext, id: &'static str| {
            harness.with_window(cx, |window, cx| window.click(id, cx));
        };
        let shows = |cx: &mut TestAppContext, id: &'static str| {
            harness.with_window(cx, |window, _| window.try_find(id).is_some())
        };
        click(cx, "chat-agents-map-3");
        assert_eq!(
            label(&harness, cx, "chat-agents-card-title"),
            "Count the words"
        );
        click(cx, "chat-agents-card-back");
        assert!(!shows(cx, "chat-agents-card"));
        click(cx, "chat-agents-map-main");
        assert!(shows(cx, "chat-agents-card"), "the conversation's own");
        click(cx, "chat-agents-card-back");
        click(cx, "chat-agents-map-3");
        click(cx, "chat-agents-card-transcript");
        assert_eq!(label(&harness, cx, "chat-agent-detail"), "Count the words");
        for (gone, left) in [
            ("chat-agent-detail", "chat-agents-card"),
            ("chat-agents-card", "chat-agents-map"),
            ("chat-agents-map", "chat-agents-list"),
        ] {
            harness.with_window(cx, |window, cx| window.press("escape", cx));
            assert!(!shows(cx, gone) && shows(cx, left), "back to {left}");
        }
        click(cx, "chat-agents-map-show");
        click(cx, "chat-agents-list-show");
        assert!(shows(cx, "chat-agents-list"), "the map's own way back");
        harness.with_window(cx, |window, cx| window.press("escape", cx));
        assert!(!shows(cx, "chat-agents-dialog"));
    }

    #[gpui_kit::test]
    fn an_agent_is_stopped_from_its_card(cx: &mut TestAppContext) {
        let harness = started(cx);
        for id in [
            "chat-agents",
            "chat-agents-map-show",
            "chat-agents-map-2",
            "chat-agents-card-stop",
        ] {
            harness.with_window(cx, |window, cx| window.click(id, cx));
        }
        // Stopping, so a second press does nothing.
        harness.with_window(cx, |window, cx| window.click("chat-agents-card-stop", cx));
        assert_eq!(
            harness.commands(),
            [SessionCommand::StopSubagent("agent".into())]
        );
        harness.emit(cx, change(SubagentChange::StopFailed));
        harness.with_window(cx, |window, cx| window.click("chat-agents-card-stop", cx));
        assert_eq!(harness.commands().len(), 1, "it can be tried again");
        harness.emit(cx, stopped());
        harness.with_window(cx, |window, _| {
            assert!(window.try_find("chat-agents-card-stop").is_none());
        });
    }

    fn stopped() -> AgentEvent {
        change(SubagentChange::Finished {
            end: SubagentEnd::Stopped,
            summary: String::new(),
            usage: None,
        })
    }

    #[gpui_kit::test]
    fn stop_reaches_agents_that_outlive_their_turn(cx: &mut TestAppContext) {
        let harness = started(cx);
        harness.emit(cx, AgentEvent::TurnEnded(TurnOutcome::Completed));
        assert_eq!(harness.status(cx), Status::Idle);
        harness.commands();
        harness.with_window(cx, |window, cx| window.click("chat-stop", cx));
        assert_eq!(harness.commands(), [SessionCommand::Interrupt]);
        harness.with_window(cx, |window, cx| window.click("chat-composer", cx));
        harness.with_window(cx, |window, cx| window.input("go on", cx));
        harness.with_window(cx, |window, _| {
            assert!(window.try_find("chat-stop").is_none(), "a message is sent");
        });
        harness.with_window(cx, |window, cx| window.press("enter", cx));
        harness.emit(cx, stopped());
        harness.emit(cx, AgentEvent::TurnEnded(TurnOutcome::Completed));
        harness.with_window(cx, |window, _| {
            assert!(window.try_find("chat-stop").is_none());
        });
    }

    #[gpui_kit::test]
    fn the_lists_width_is_dragged_and_kept(cx: &mut TestAppContext) {
        let harness = started(cx);
        harness.with_window(cx, |window, cx| window.click("chat-agents", cx));
        let list = |cx: &mut TestAppContext| {
            harness.with_window(cx, |window, _| window.find("chat-agents-list").bounds())
        };
        let before = list(cx);
        let edge = point(before.right(), before.center().y);
        let wider = point(edge.x + px(60.), edge.y);
        harness.with_window(cx, |window, cx| window.drag(edge, wider, cx));
        let dragged = list(cx).size.width;
        assert!(dragged > before.size.width + px(40.), "{dragged:?}");
        harness.with_window(cx, |window, cx| window.press("escape", cx));
        harness.with_window(cx, |window, cx| window.click("chat-agents", cx));
        let kept = list(cx).size.width;
        assert!((kept - dragged).abs() < px(2.), "{kept:?}, not {dragged:?}");
    }
}
