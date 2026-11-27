//! Running a chat on Claude Code or Codex: a blank chat switches between
//! them, a stored conversation opens on the one that stored it, and a
//! missing CLI is offered for installing.

// Inside a `cfg(test)` module so helpers may `expect`, like unit tests.
#[cfg(test)]
mod codex;
#[cfg(test)]
mod common;

#[cfg(test)]
mod agents {
    use std::{cell::RefCell, rc::Rc};

    use gpui_kit::TestAppContext;
    use gpui_kit::test::{TestQueryExt as _, TestWindowExt as _};
    use vortex_agent::{AgentEvent, PastMessage, TurnOutcome};
    use vortex_chat::{Status, Stored};

    use super::codex::{agent, click, offering_codex, shown};
    use super::common::Harness;

    #[gpui_kit::test]
    fn a_chat_with_one_agent_offers_no_other(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        assert!(!shown(&harness, cx, "chat-agent-picker"));
        assert_eq!(
            cx.update(|cx| harness.view.read(cx).agent_names(cx)),
            ["claude"]
        );
    }

    #[gpui_kit::test]
    fn a_blank_chat_moves_to_codex_with_a_process_of_its_own(cx: &mut TestAppContext) {
        let (harness, codex) = offering_codex(cx);
        assert_eq!(agent(&harness, cx), "claude");
        assert!(shown(&harness, cx, "chat-agent-codex"));
        assert!(shown(&harness, cx, "chat-skills"));
        harness.with_window(cx, |window, cx| {
            window.click("chat-agent-codex", cx);
        });
        assert_eq!(agent(&harness, cx), "codex");
        for hidden in ["chat-skills", "chat-hooks", "chat-cache"] {
            assert!(!shown(&harness, cx, hidden), "Codex has no {hidden}");
        }
        assert_eq!(codex.borrow().spawned, 1, "Codex runs it now");
        let names = cx.update(|cx| harness.view.read(cx).agent_names(cx));
        assert_eq!(names, ["codex", "claude"], "its own agent first");
    }

    #[gpui_kit::test]
    fn a_chat_that_said_something_keeps_its_agent(cx: &mut TestAppContext) {
        let (harness, codex) = offering_codex(cx);
        harness.type_and_send(cx, "hello");
        assert_eq!(harness.commands().len(), 1, "sent to Claude");
        harness.emit(cx, AgentEvent::TurnEnded(TurnOutcome::Completed));
        assert_eq!(harness.status(cx), Status::Idle);
        assert!(!shown(&harness, cx, "chat-agent-codex"));
        harness.with_window(cx, |window, cx| {
            harness
                .view
                .update(cx, |view, cx| view.use_agent("codex", window, cx));
        });
        assert_eq!(agent(&harness, cx), "claude");
        assert_eq!(codex.borrow().spawned, 0);
    }

    #[gpui_kit::test]
    fn a_conversation_opens_on_the_agent_that_stored_it(cx: &mut TestAppContext) {
        let (harness, codex) = offering_codex(cx);
        codex.borrow_mut().stored = vec![("t-1".into(), vec![PastMessage::User("Hi".into())])];
        let spawned = harness.script.borrow().spawned;
        harness.with_window(cx, |window, cx| {
            harness.view.update(cx, |view, cx| {
                let stored = Stored {
                    agent: "codex",
                    id: "t-1",
                    open_elsewhere: false,
                };
                view.open_session(&stored, window, cx);
            });
        });
        assert_eq!(agent(&harness, cx), "codex");
        let script = codex.borrow();
        let spec = script.last_spec.as_ref().expect("Codex resumes it");
        assert_eq!(spec.resume.as_deref(), Some("t-1"));
        drop(script);
        assert_eq!(
            harness.script.borrow().spawned,
            spawned,
            "Claude is not asked"
        );
        assert!(!shown(&harness, cx, "chat-agent-claude"), "nor offered");
    }

    #[gpui_kit::test]
    fn each_agent_starts_with_the_model_last_picked_on_it(cx: &mut TestAppContext) {
        let (harness, codex) = offering_codex(cx);
        cx.update(|cx| {
            harness
                .view
                .update(cx, |view, cx| view.set_model("opus", cx));
        });
        click(&harness, cx, "chat-agent-codex");
        let spec = codex.borrow().last_spec.clone().expect("Codex runs");
        assert_eq!(spec.model, None, "nothing picked on Codex yet");
        cx.update(|cx| {
            harness
                .view
                .update(cx, |view, cx| view.set_model("gpt-6-luna", cx));
        });
        click(&harness, cx, "chat-agent-claude");
        let spec = harness
            .script
            .borrow()
            .last_spec
            .clone()
            .expect("Claude runs");
        assert_eq!(spec.model.as_deref(), Some("opus"));
        // A new chat, as a new tab or after a restart, starts on it too.
        let fresh = Harness::new(cx);
        let spec = fresh
            .script
            .borrow()
            .last_spec
            .clone()
            .expect("Claude runs");
        assert_eq!(spec.model.as_deref(), Some("opus"));
        let kept = cx.update(|cx| vortex_chat::LastModels::of("codex", cx));
        assert_eq!(kept.as_deref(), Some("gpt-6-luna"));
    }

    #[gpui_kit::test]
    fn a_codex_subagent_counts_while_it_runs(cx: &mut TestAppContext) {
        use vortex_agent::{SubagentChange, SubagentEnd, SubagentEvent, ToolCall};
        let (harness, codex) = offering_codex(cx);
        click(&harness, cx, "chat-agent-codex");
        harness.type_and_send(cx, "spawn a subagent");
        let events = codex.borrow().events.clone().expect("Codex runs");
        let send = |cx: &mut TestAppContext, event: AgentEvent| {
            events.try_send(event).expect("send");
            cx.run_until_parked();
        };
        let subagent = |change| {
            AgentEvent::Subagent(SubagentEvent {
                call: "call-1".into(),
                change,
            })
        };
        // As the Codex translator reports a subagent's start.
        send(
            cx,
            AgentEvent::ToolCall(ToolCall {
                id: "call-1".into(),
                parent: None,
                name: "Agent".into(),
                input: serde_json::json!({ "description": "heron word" }),
            }),
        );
        send(
            cx,
            subagent(SubagentChange::Started {
                description: "heron word".into(),
                kind: None,
            }),
        );
        let counted = |cx: &mut TestAppContext, label: &'static str| {
            harness.with_window(cx, move |window, _| {
                window.try_find_by_label(label).is_some()
            })
        };
        assert!(counted(cx, "Agents 1"));
        send(
            cx,
            subagent(SubagentChange::Finished {
                end: SubagentEnd::Completed,
                summary: "heron".into(),
                usage: None,
            }),
        );
        assert!(counted(cx, "Agents 0"));
    }

    #[gpui_kit::test]
    fn the_accounts_panel_offers_a_way_to_a_missing_harness(cx: &mut TestAppContext) {
        let (harness, _codex) = offering_codex(cx);
        cx.update(|cx| {
            vortex_chat::AgentSetup::set("codex", Some(vortex_chat::Setup::Missing), cx);
        });
        let asked = Rc::new(RefCell::new(Vec::new()));
        let seen = asked.clone();
        let _watch = cx.update(|cx| {
            cx.subscribe(
                &harness.view,
                move |_, event: &vortex_chat::ChatEvent, _| {
                    if let vortex_chat::ChatEvent::ChooseAgentPath(agent) = event {
                        seen.borrow_mut().push(*agent);
                    }
                },
            )
        });
        click(&harness, cx, "chat-account");
        assert!(shown(&harness, cx, "chat-account-missing-codex"));
        click(&harness, cx, "chat-account-choose-codex");
        assert_eq!(*asked.borrow(), ["codex"]);
    }

    #[gpui_kit::test]
    fn a_missing_agent_offers_to_install_it_and_starts_once_it_is_there(cx: &mut TestAppContext) {
        let (harness, codex) = offering_codex(cx);
        codex.borrow_mut().missing = true;
        let asked = Rc::new(RefCell::new(Vec::new()));
        let seen = asked.clone();
        let _watch = cx.update(|cx| {
            cx.subscribe(
                &harness.view,
                move |_, event: &vortex_chat::ChatEvent, _| {
                    if let vortex_chat::ChatEvent::InstallAgent(agent) = event {
                        seen.borrow_mut().push(*agent);
                    }
                },
            )
        });
        click(&harness, cx, "chat-agent-codex");
        assert!(shown(&harness, cx, "chat-install"), "in place of an error");
        assert!(cx.update(|cx| vortex_chat::AgentSetup::is_missing("codex", cx)));
        click(&harness, cx, "chat-install-agent");
        assert_eq!(*asked.borrow(), ["codex"]);

        codex.borrow_mut().missing = false;
        cx.update(|cx| vortex_chat::AgentSetup::set("codex", None, cx));
        harness.with_window(cx, |window, cx| {
            harness
                .view
                .update(cx, |view, cx| view.cli_checked("codex", window, cx));
        });
        assert_eq!(codex.borrow().spawned, 1, "it starts once installed");
        assert!(!shown(&harness, cx, "chat-install"));
    }

    #[gpui_kit::test]
    fn a_missing_other_agent_is_not_offered(cx: &mut TestAppContext) {
        let (harness, _codex) = offering_codex(cx);
        assert!(shown(&harness, cx, "chat-agent-codex"));
        cx.update(|cx| {
            vortex_chat::AgentSetup::set("codex", Some(vortex_chat::Setup::Missing), cx);
        });
        harness.with_window(cx, |window, cx| {
            harness
                .view
                .update(cx, |view, cx| view.cli_checked("codex", window, cx));
        });
        assert!(!shown(&harness, cx, "chat-agent-codex"));
        let names = cx.update(|cx| harness.view.read(cx).agent_names(cx));
        assert_eq!(names, ["claude"]);
    }

    #[gpui_kit::test]
    fn codex_starting_a_conversation_before_any_message_keeps_the_chat_blank(
        cx: &mut TestAppContext,
    ) {
        let (harness, codex) = offering_codex(cx);
        click(&harness, cx, "chat-agent-codex");
        let events = codex.borrow().events.clone().expect("Codex runs");
        let started = AgentEvent::Started {
            session_id: "t-1".into(),
            model: "gpt-6-luna".into(),
            cli_version: Some("0.172.0".into()),
        };
        events.try_send(started).expect("send");
        cx.run_until_parked();
        assert!(cx.update(|cx| harness.view.read(cx).is_blank()));
        click(&harness, cx, "chat-agent-claude");
        assert_eq!(agent(&harness, cx), "claude", "it still moves");
        let view = |cx: &mut TestAppContext| {
            cx.update(|cx| {
                let chat = harness.view.read(cx);
                (
                    chat.session_id().map(str::to_owned),
                    chat.cli_version().map(str::to_owned),
                )
            })
        };
        assert_eq!(
            view(cx),
            (None, None),
            "Codex's id and version stay Codex's"
        );
    }
}
