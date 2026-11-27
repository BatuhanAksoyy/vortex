//! Adding accounts to a chat through its accounts panel.

use std::path::Path;

use gpui_kit::TestAppContext;
use gpui_kit::test::{TestQueryExt, TestWindowExt};
use vortex_agent::{Account, AgentEvent, SessionInfo};

use super::common::Harness;

pub const SIGN_IN_PAGE: &str = "https://claude.com/cai/oauth/authorize?test";

pub fn ready(cx: &mut TestAppContext) -> Harness {
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

pub fn account(email: &str, plan: &str) -> Account {
    Account {
        email: email.into(),
        organization: String::new(),
        plan: plan.into(),
    }
}

pub fn click(harness: &Harness, cx: &mut TestAppContext, id: impl Into<String>) {
    let id = id.into();
    harness.with_window(cx, move |window, cx| window.click(id, cx));
}

pub fn shown(harness: &Harness, cx: &mut TestAppContext, label: &str) -> bool {
    let label = label.to_owned();
    harness.with_window(cx, move |window, _| {
        window.try_find_by_label(&label).is_some()
    })
}

/// Starts a sign-in from the open panel; returns the sign-in's
/// credential store, named after the account's id.
pub fn start_sign_in(harness: &Harness, cx: &mut TestAppContext) -> String {
    click(harness, cx, "chat-account-add");
    let script = harness.script.borrow();
    let (page, _, env) = script.sign_in.as_ref().expect("a sign-in started");
    let (var, store) = env.first().cloned().expect("a credential store");
    assert_eq!(var, "CLAUDE_SECURESTORAGE_CONFIG_DIR");
    page.try_send(SIGN_IN_PAGE.into()).expect("send page");
    store
}

pub fn finish_sign_in(harness: &Harness, cx: &mut TestAppContext, result: anyhow::Result<Account>) {
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
pub fn add(harness: &Harness, cx: &mut TestAppContext, email: &str) -> String {
    let store = start_sign_in(harness, cx);
    finish_sign_in(harness, cx, Ok(account(email, "Claude Max")));
    Path::new(&store)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .expect("an id")
}
