//! Running a chat on Claude Code or Codex: a blank chat switches between
//! them, and a stored conversation opens on the one that stored it.

// Inside a `cfg(test)` module so helpers may `expect`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod agents {
    use std::{cell::RefCell, path::Path, rc::Rc};

    use gpui_kit::TestAppContext;
    use gpui_kit::test::{TestQueryExt as _, TestWindowExt as _};
    use kiln_agent::{
        AgentBackend, AgentEvent, Features, PastMessage, PastSession, Pending, Session,
        SessionSpec, SignIn, TurnOutcome,
    };
    use kiln_chat::{Status, Stored};

    use super::common::{FakeBackend, Harness, Script};

    /// The scripted backend under Codex's name.
    struct Codex(FakeBackend);

    impl AgentBackend for Codex {
        fn name(&self) -> &'static str {
            "codex"
        }

        fn features(&self) -> Features {
            Features {
                skills_and_hooks: false,
                prompt_cache: false,
            }
        }

        fn spawn(&self, spec: SessionSpec) -> anyhow::Result<Session> {
            self.0.spawn(spec)
        }

        fn past_sessions(&self, cwd: &Path) -> Pending<Vec<PastSession>> {
            self.0.past_sessions(cwd)
        }

        fn past_messages(&self, cwd: &Path, id: &str) -> Pending<Vec<PastMessage>> {
            self.0.past_messages(cwd, id)
        }

        fn sign_in(&self, env: Vec<(String, String)>) -> anyhow::Result<SignIn> {
            self.0.sign_in(env)
        }
    }

    /// A chat that also offers Codex, and the script of Codex's sessions.
    fn offering_codex(cx: &mut TestAppContext) -> (Harness, Rc<RefCell<Script>>) {
        let codex = Rc::new(RefCell::new(Script::default()));
        let backend: Rc<dyn AgentBackend> = Rc::new(Codex(FakeBackend(codex.clone())));
        (Harness::with(cx, Some(backend)), codex)
    }

    fn shown(harness: &Harness, cx: &mut TestAppContext, id: &'static str) -> bool {
        harness.with_window(cx, move |window, _| window.try_find(id).is_some())
    }

    fn agent(harness: &Harness, cx: &mut TestAppContext) -> &'static str {
        cx.update(|cx| harness.view.read(cx).agent_name())
    }

    #[gpui_kit::test]
    fn a_chat_with_one_agent_offers_no_other(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        assert!(!shown(&harness, cx, "chat-agent-picker"));
        assert_eq!(
            cx.update(|cx| harness.view.read(cx).agent_names()),
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
        let names = cx.update(|cx| harness.view.read(cx).agent_names());
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

    fn click(harness: &Harness, cx: &mut TestAppContext, id: &'static str) {
        harness.with_window(cx, |window, cx| window.click(id, cx));
    }

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
                block: kiln_agent::BlockKey {
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
        let limited = AgentEvent::Usage(kiln_agent::Usage {
            limited: Some(kiln_agent::Limited {
                until: Some(std::time::SystemTime::now() + std::time::Duration::from_hours(1)),
            }),
            ..kiln_agent::Usage::default()
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
                kiln_agent::SessionCommand::Send(message) if message.text == "Continue where you left off."
            )),
            "{sent:?}"
        );
    }
}
