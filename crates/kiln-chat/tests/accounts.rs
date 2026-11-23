//! The header's accounts panel: adding an account by signing in through the
//! browser, moving the conversation to another account, and carrying on
//! with another account at a usage limit.

// Inside a `cfg(test)` module so helpers may `expect`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod accounts {
    use std::{
        path::Path,
        time::{Duration, SystemTime},
    };

    use gpui_kit::test::{TestQueryExt, TestWindowExt};
    use gpui_kit::{AppContext as _, TestAppContext};
    use kiln_agent::{
        Account, AgentEvent, Limited, SessionCommand, SessionInfo, TurnOutcome, Usage, UsageWindow,
    };
    use kiln_chat::Status;

    use super::common::{self, Harness};

    const SIGN_IN_PAGE: &str = "https://claude.com/cai/oauth/authorize?test";

    fn ready(cx: &mut TestAppContext) -> Harness {
        let harness = Harness::new(cx);
        harness.emit(
            cx,
            AgentEvent::Ready(SessionInfo {
                account: Some(account("dev@example.com", "Claude Pro")),
                ..SessionInfo::default()
            }),
        );
        harness.commands();
        harness
    }

    fn account(email: &str, plan: &str) -> Account {
        Account {
            email: email.into(),
            organization: String::new(),
            plan: plan.into(),
        }
    }

    fn click(harness: &Harness, cx: &mut TestAppContext, id: impl Into<String>) {
        let id = id.into();
        harness.with_window(cx, move |window, cx| window.click(id, cx));
    }

    fn shown(harness: &Harness, cx: &mut TestAppContext, label: &str) -> bool {
        let label = label.to_owned();
        harness.with_window(cx, move |window, _| {
            window.try_find_by_label(&label).is_some()
        })
    }

    /// Starts a sign-in from the open panel; returns the sign-in's
    /// credential store, named after the account's id.
    fn start_sign_in(harness: &Harness, cx: &mut TestAppContext) -> String {
        click(harness, cx, "chat-account-add");
        let script = harness.script.borrow();
        let (page, _, env) = script.sign_in.as_ref().expect("a sign-in started");
        let (var, store) = env.first().cloned().expect("a credential store");
        assert_eq!(var, "CLAUDE_SECURESTORAGE_CONFIG_DIR");
        page.try_send(SIGN_IN_PAGE.into()).expect("send page");
        store
    }

    fn finish_sign_in(harness: &Harness, cx: &mut TestAppContext, result: anyhow::Result<Account>) {
        let done = harness
            .script
            .borrow()
            .sign_in
            .as_ref()
            .map(|(_, done, _)| done.clone());
        done.expect("a sign-in started")
            .try_send(result)
            .expect("finish sign-in");
        cx.run_until_parked();
    }

    /// Adds `email` through the panel; returns its id.
    fn add(harness: &Harness, cx: &mut TestAppContext, email: &str) -> String {
        let store = start_sign_in(harness, cx);
        finish_sign_in(harness, cx, Ok(account(email, "Claude Max")));
        Path::new(&store)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .expect("an id")
    }

    /// The environment the latest process was started with.
    fn last_env(harness: &Harness) -> Vec<(String, String)> {
        let script = harness.script.borrow();
        script.last_spec.as_ref().expect("spawned").env.clone()
    }

    fn store_of(env: &[(String, String)]) -> String {
        let (_, store) = env.first().cloned().expect("a credential store");
        store
    }

    fn limited() -> AgentEvent {
        AgentEvent::Usage(Usage {
            limited: Some(Limited {
                until: Some(SystemTime::now() + Duration::from_hours(1)),
            }),
            ..Usage::default()
        })
    }

    #[gpui_kit::test]
    fn without_an_added_account_the_usual_sign_in_is_used(cx: &mut TestAppContext) {
        let harness = ready(cx);
        assert_eq!(last_env(&harness), Vec::new());
        assert!(
            shown(&harness, cx, "dev@example.com · Pro"),
            "the header names it"
        );
        click(&harness, cx, "chat-account");
        assert!(harness.with_window(cx, |window, _| {
            window.try_find("chat-account-usual").is_some()
        }));
        assert!(shown(&harness, cx, "Add account"));
    }

    #[gpui_kit::test]
    fn an_account_is_added_by_signing_in_in_the_browser(cx: &mut TestAppContext) {
        let harness = ready(cx);
        click(&harness, cx, "chat-account");
        let store = start_sign_in(&harness, cx);
        cx.run_until_parked();
        assert_eq!(cx.opened_url().as_deref(), Some(SIGN_IN_PAGE));
        assert!(shown(&harness, cx, "Finish signing in in your browser."));
        assert!(Path::new(&store).is_dir());
        finish_sign_in(
            &harness,
            cx,
            Ok(account("second@example.com", "Claude Max")),
        );
        assert!(shown(&harness, cx, "Added second@example.com."));
        assert!(shown(&harness, cx, "second@example.com"));
        assert!(shown(&harness, cx, "Add account"), "ready for another");
    }

    #[gpui_kit::test]
    fn the_first_added_account_takes_over_with_the_session(cx: &mut TestAppContext) {
        let harness = ready(cx);
        harness.type_and_send(cx, "one");
        harness.emit(
            cx,
            AgentEvent::Started {
                session_id: "s-1".into(),
                model: "claude-opus-5-5".into(),
            },
        );
        harness.emit(cx, AgentEvent::TurnEnded(TurnOutcome::Completed));
        click(&harness, cx, "chat-account");
        let id = add(&harness, cx, "second@example.com");
        let spec = harness.script.borrow().last_spec.clone().expect("spawned");
        assert_eq!(
            Path::new(&store_of(&spec.env)).file_name(),
            Some(id.as_ref())
        );
        assert_eq!(spec.resume.as_deref(), Some("s-1"), "in the same session");
        assert_eq!(
            harness.status(cx),
            Status::Idle,
            "ready for the next message"
        );
        assert!(shown(&harness, cx, "Switched to second@example.com"));
        assert!(shown(&harness, cx, "second@example.com · Max"));
    }

    #[gpui_kit::test]
    fn an_account_added_mid_turn_takes_over_when_the_turn_ends(cx: &mut TestAppContext) {
        let harness = ready(cx);
        harness.type_and_send(cx, "one");
        assert_eq!(harness.status(cx), Status::Running);
        click(&harness, cx, "chat-account");
        add(&harness, cx, "second@example.com");
        assert_eq!(last_env(&harness), Vec::new(), "the turn is not cut off");
        harness.emit(cx, AgentEvent::TurnEnded(TurnOutcome::Completed));
        assert_ne!(last_env(&harness), Vec::new());
    }

    #[gpui_kit::test]
    fn another_added_account_is_picked_in_the_panel(cx: &mut TestAppContext) {
        let harness = ready(cx);
        click(&harness, cx, "chat-account");
        let first = add(&harness, cx, "first@example.com");
        let second = add(&harness, cx, "second@example.com");
        assert!(
            store_of(&last_env(&harness)).ends_with(&first),
            "still the first"
        );
        click(&harness, cx, format!("chat-account-{second}"));
        assert!(store_of(&last_env(&harness)).ends_with(&second));
        assert!(shown(&harness, cx, "Switched to second@example.com"));
    }

    #[gpui_kit::test]
    fn an_added_account_reads_its_usage_from_its_own_turns(cx: &mut TestAppContext) {
        let harness = ready(cx);
        click(&harness, cx, "chat-account");
        add(&harness, cx, "second@example.com");
        harness.emit(cx, AgentEvent::Ready(SessionInfo::default()));
        harness.emit(cx, AgentEvent::TurnEnded(TurnOutcome::Completed));
        let asked = harness.commands();
        assert!(asked.contains(&SessionCommand::RefreshContext));
        assert!(!asked.contains(&SessionCommand::RefreshUsage), "{asked:?}");
    }

    #[gpui_kit::test]
    fn at_the_limit_the_chat_continues_with_another_account(cx: &mut TestAppContext) {
        let harness = ready(cx);
        click(&harness, cx, "chat-account");
        let first = add(&harness, cx, "first@example.com");
        let second = add(&harness, cx, "second@example.com");
        harness.emit(cx, limited());
        assert!(shown(&harness, cx, "Continue with second@example.com"));
        click(&harness, cx, "chat-limit-continue");
        assert!(store_of(&last_env(&harness)).ends_with(&second));
        assert!(harness.with_window(cx, |window, _| window.try_find("chat-limit").is_none()));
        harness.emit(cx, limited());
        let offered = harness.with_window(cx, |window, _| window.try_find("chat-limit-continue"));
        assert!(offered.is_none(), "{first} is limited too");
    }

    #[gpui_kit::test]
    fn the_usual_sign_in_is_offered_nothing_at_the_limit(cx: &mut TestAppContext) {
        let harness = ready(cx);
        harness.emit(cx, limited());
        assert!(harness.with_window(cx, |window, _| window.try_find("chat-limit").is_some()));
        let offered = harness.with_window(cx, |window, _| window.try_find("chat-limit-continue"));
        assert!(offered.is_none());
    }

    #[gpui_kit::test]
    fn an_account_already_added_is_not_added_twice(cx: &mut TestAppContext) {
        let harness = ready(cx);
        click(&harness, cx, "chat-account");
        add(&harness, cx, "second@example.com");
        let store = start_sign_in(&harness, cx);
        finish_sign_in(
            &harness,
            cx,
            Ok(account("Second@example.com", "Claude Max")),
        );
        assert!(shown(&harness, cx, "Second@example.com is already added."));
        assert!(
            !Path::new(&store).exists(),
            "its credential store is dropped"
        );
        let rows = harness.with_window(cx, |window, _| {
            window.find_all_by_label("second@example.com").len()
        });
        assert_eq!(rows, 1);
    }

    #[gpui_kit::test]
    fn a_failed_sign_in_says_why_and_leaves_nothing_behind(cx: &mut TestAppContext) {
        let harness = ready(cx);
        click(&harness, cx, "chat-account");
        let store = start_sign_in(&harness, cx);
        finish_sign_in(&harness, cx, Err(anyhow::anyhow!("access denied")));
        assert!(shown(&harness, cx, "The sign-in failed: access denied"));
        assert!(!Path::new(&store).exists());
    }

    #[gpui_kit::test]
    fn cancel_ends_the_sign_in(cx: &mut TestAppContext) {
        let harness = ready(cx);
        click(&harness, cx, "chat-account");
        let store = start_sign_in(&harness, cx);
        click(&harness, cx, "chat-account-cancel");
        let done = harness
            .script
            .borrow()
            .sign_in
            .as_ref()
            .map(|(_, done, _)| done.clone());
        assert!(
            done.is_some_and(|done| done.is_closed()),
            "the CLI is let go"
        );
        assert!(!Path::new(&store).exists());
        assert!(shown(&harness, cx, "Add account"));
    }

    #[gpui_kit::test]
    fn an_added_accounts_last_usage_shows_in_a_new_chat_and_is_kept(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        let data = tempfile::tempdir().expect("temp dir");
        let mut kept = kiln_accounts::Accounts::load(data.path().to_owned()).expect("list");
        let (id, _) = kept.reserve().expect("reserve");
        kept.add(kiln_accounts::Account {
            id: id.clone(),
            email: "a@example.com".into(),
            plan: "Claude Pro".into(),
        })
        .expect("add");
        let session = |percent| Usage {
            session: Some(UsageWindow {
                percent,
                resets_at: None,
            }),
            ..Usage::default()
        };
        let known = kiln_accounts::KnownUsage {
            usage: session(42),
            reported_at: SystemTime::now(),
        };
        kept.set_usage(&id, known).expect("keep usage");
        // As after a restart: the list and its usage come from disk.
        let path = data.path().to_owned();
        let backend = std::rc::Rc::new(common::FakeBackend(harness.script.clone()));
        let (window, _) = cx.update(|cx| {
            let accounts = cx.new(|_| kiln_accounts::Accounts::load(path).expect("list"));
            common::open(backend, accounts, cx)
        });
        cx.run_until_parked();
        let meter = |cx: &mut TestAppContext, label: &'static str| {
            cx.update_window(window, |_, window, cx| {
                window.render_frame(cx);
                window.try_find_by_label(label).is_some()
            })
            .expect("window")
        };
        assert!(meter(cx, "42% used"), "before the account's first reply");

        harness.emit(cx, AgentEvent::Usage(session(57)));
        assert!(meter(cx, "57% used"));
        let reloaded = kiln_accounts::Accounts::load(data.path().to_owned()).expect("list");
        let stored = reloaded.usage(&id).expect("kept");
        assert_eq!(stored.usage.session.map(|w| w.percent), Some(57));
    }
}
