//! Handing a conversation over between Claude Code and Codex: from the
//! accounts panel, and at the usage limit.

// Inside a `cfg(test)` module so helpers may `expect`, like unit tests.
#[cfg(test)]
mod codex;
#[cfg(test)]
mod common;

#[cfg(test)]
mod handoff {
    use std::{cell::RefCell, rc::Rc};

    use gpui_kit::TestAppContext;
    use gpui_kit::test::TestQueryExt as _;
    use vortex_agent::{AgentEvent, TurnOutcome};
    use vortex_chat::Status;

    use super::codex::{agent, click, offering_codex, shown};
    use super::common::{Harness, Script};

    /// A Claude chat that said hello and was answered.
    fn answered(cx: &mut TestAppContext) -> (Harness, Rc<RefCell<Script>>) {
        let (harness, codex) = offering_codex(cx);
        harness.type_and_send(cx, "hello");
        harness.emit(
            cx,
            AgentEvent::Started {
                session_id: "s-1".into(),
                model: "claude-opus-5-5".into(),
                cli_version: None,
            },
        );
        harness.emit(
            cx,
            AgentEvent::TextDelta {
                block: vortex_agent::BlockKey {
                    message_id: "m-1".into(),
                    index: 0,
                },
                text: "Hi there".into(),
            },
        );
        harness.emit(cx, AgentEvent::TurnEnded(TurnOutcome::Completed));
        (harness, codex)
    }

    #[gpui_kit::test]
    fn both_agents_accounts_show_in_one_panel(cx: &mut TestAppContext) {
        let (harness, _codex) = offering_codex(cx);
        click(&harness, cx, "chat-account");
        for id in [
            "chat-account-add",
            "chat-account-add-codex",
            "chat-account-usual-codex",
        ] {
            assert!(shown(&harness, cx, id), "{id}");
        }
    }

    #[gpui_kit::test]
    fn a_conversation_carries_on_in_codex_and_back(cx: &mut TestAppContext) {
        let (harness, codex) = answered(cx);
        click(&harness, cx, "chat-account");
        click(&harness, cx, "chat-account-usual-codex");
        assert_eq!(agent(&harness, cx), "codex");
        let script = codex.borrow();
        let spec = script.last_spec.as_ref().expect("Codex takes it over");
        assert_eq!(spec.resume, None, "as a conversation of its own");
        let told = spec.handoff.as_deref().expect("told the conversation");
        assert!(told.contains("User: hello"), "{told}");
        assert!(told.contains("Assistant: Hi there"), "{told}");
        drop(script);
        assert!(
            harness.with_window(cx, |window, _| {
                window.try_find_by_label("Continued in Codex").is_some()
            }),
            "the transcript says so"
        );
        assert_eq!(harness.status(cx), Status::Idle);

        let events = codex.borrow().events.clone().expect("Codex runs");
        let started = AgentEvent::Started {
            session_id: "t-1".into(),
            model: "gpt-6-luna".into(),
            cli_version: None,
        };
        events.try_send(started).expect("send");
        cx.run_until_parked();
        let id = cx.update(|cx| harness.view.read(cx).session_id().map(str::to_owned));
        assert_eq!(id.as_deref(), Some("t-1"));

        let spawned = harness.script.borrow().spawned;
        click(&harness, cx, "chat-account");
        click(&harness, cx, "chat-account-usual-claude");
        assert_eq!(agent(&harness, cx), "claude");
        let script = harness.script.borrow();
        assert_eq!(script.spawned, spawned + 1);
        let spec = script.last_spec.as_ref().expect("Claude takes it back");
        assert_eq!(spec.resume, None);
        assert!(
            spec.handoff
                .as_deref()
                .is_some_and(|told| told.contains("User: hello"))
        );
    }

    #[gpui_kit::test]
    fn at_the_limit_with_no_other_account_codex_carries_the_turn_on(cx: &mut TestAppContext) {
        let (harness, codex) = answered(cx);
        harness.type_and_send(cx, "write the parser");
        let limited = AgentEvent::Usage(vortex_agent::Usage {
            limited: Some(vortex_agent::Limited {
                until: Some(std::time::SystemTime::now() + std::time::Duration::from_hours(1)),
            }),
            ..vortex_agent::Usage::default()
        });
        harness.emit(cx, limited);
        let failed = TurnOutcome::Failed("usage limit reached".into());
        harness.emit(cx, AgentEvent::TurnEnded(failed));
        assert!(harness.with_window(cx, |window, _| {
            window.try_find_by_label("Continue in Codex").is_some()
        }));
        click(&harness, cx, "chat-limit-continue");
        assert_eq!(agent(&harness, cx), "codex");
        let script = codex.borrow();
        let told = script
            .last_spec
            .as_ref()
            .and_then(|spec| spec.handoff.clone());
        assert!(told.is_some_and(|told| told.contains("User: write the parser")));
        let commands = script.commands.as_ref().expect("Codex runs");
        let sent: Vec<_> = std::iter::from_fn(|| commands.try_recv().ok()).collect();
        assert!(
            sent.iter().any(|command| matches!(
                command,
                vortex_agent::SessionCommand::Send(message) if message.text == "Continue where you left off."
            )),
            "{sent:?}"
        );
    }

    #[gpui_kit::test]
    fn a_chat_with_one_harness_offers_no_other_in_its_panel(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        click(&harness, cx, "chat-account");
        assert!(shown(&harness, cx, "chat-account-add"));
        assert!(!shown(&harness, cx, "chat-account-add-codex"));
        harness.type_and_send(cx, "hello");
        assert_eq!(harness.commands().len(), 1, "the message goes to Claude");
    }
}
