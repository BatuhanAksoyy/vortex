//! The chat layout kept across restarts: tabs, splits and the conversations
//! in them come back.

// Inside a `cfg(test)` module so helpers may `expect`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod layouts {
    use gpui_kit::TestAppContext;

    use gpui_kit::{AnyWindowHandle, AppContext as _, test::TestWindowExt as _};
    use kiln_accounts::{Account, Accounts};
    use kiln_agent::PermissionMode;

    use super::common::{Stored, chats, click, labels, no_accounts, open_in, pick, spawned};

    fn resumed(backend: &Stored) -> Vec<String> {
        let specs = backend.0.borrow();
        specs
            .iter()
            .filter_map(|spec| spec.resume.clone())
            .collect()
    }

    #[gpui_kit::test]
    fn the_tabs_and_splits_come_back_after_a_restart(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().expect("temp dir");
        let layouts = dir.path().join("layouts.json");
        let before = open_in(
            cx,
            Stored::default(),
            Some(layouts.clone()),
            no_accounts(),
            Some(env!("CARGO_MANIFEST_DIR").into()),
        );
        pick(cx, before, "s-new");
        click(cx, before, "chat-split-right");
        pick(cx, before, "s-old");

        let backend = Stored::default();
        let after = open_in(
            cx,
            backend.clone(),
            Some(layouts),
            no_accounts(),
            Some(env!("CARGO_MANIFEST_DIR").into()),
        );
        let shown = chats(cx, after);
        assert_eq!(shown.len(), 2, "both groups");
        assert!(shown[0].origin.x < shown[1].origin.x, "side by side");
        assert_eq!(resumed(&backend), ["s-new", "s-old"]);
        assert_eq!(spawned(&backend), 2, "no extra blank chat");
        assert!(labels(cx, after, "Fix the build") > 0, "shown");
        assert!(labels(cx, after, "Plan the release") > 0, "shown");
    }

    #[gpui_kit::test]
    fn a_kept_conversation_no_longer_stored_is_left_out(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().expect("temp dir");
        let layouts = dir.path().join("layouts.json");
        let root = env!("CARGO_MANIFEST_DIR");
        let kept = serde_json::json!({
            root: {
                "kind": "tabs",
                "active": 0,
                "chats": [{"session": "s-gone"}, {"session": "s-old"}],
            }
        });
        std::fs::write(&layouts, kept.to_string()).expect("write layouts");
        let backend = Stored::default();
        let window = open_in(
            cx,
            backend.clone(),
            Some(layouts),
            no_accounts(),
            Some(env!("CARGO_MANIFEST_DIR").into()),
        );
        assert_eq!(resumed(&backend), ["s-old"]);
        assert!(labels(cx, window, "Plan the release") > 0, "shown");
    }

    /// Two added accounts kept in `dir`, and their ids.
    fn two_accounts(dir: &std::path::Path) -> (String, String) {
        let mut accounts = Accounts::load(dir.to_owned()).expect("list");
        let mut add = |email: &str| {
            let (id, _) = accounts.reserve().expect("reserve");
            let account = Account {
                id: id.clone(),
                email: email.into(),
                plan: "Claude Pro".into(),
            };
            accounts.add(account).expect("add");
            id
        };
        (add("a@example.com"), add("b@example.com"))
    }

    fn click_in(cx: &mut TestAppContext, window: AnyWindowHandle, id: String) {
        cx.update_window(window, |_, window, cx| window.click(id, cx))
            .expect("window");
        cx.run_until_parked();
    }

    #[gpui_kit::test]
    fn each_tab_comes_back_with_its_account(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().expect("temp dir");
        let layouts = dir.path().join("layouts.json");
        let (first, second) = two_accounts(dir.path());
        let accounts = || Accounts::load(dir.path().to_owned()).expect("list");
        let before = open_in(
            cx,
            Stored::default(),
            Some(layouts.clone()),
            accounts(),
            Some(env!("CARGO_MANIFEST_DIR").into()),
        );
        pick(cx, before, "s-new");
        click(cx, before, "chat-account");
        click_in(cx, before, format!("chat-account-{second}"));
        click(cx, before, "chat-split-right");
        pick(cx, before, "s-old");

        let backend = Stored::default();
        open_in(
            cx,
            backend.clone(),
            Some(layouts),
            accounts(),
            Some(env!("CARGO_MANIFEST_DIR").into()),
        );
        let env = |session: &str| {
            let specs = backend.0.borrow();
            let spec = specs
                .iter()
                .find(|spec| spec.resume.as_deref() == Some(session))
                .expect("resumed");
            spec.env.clone()
        };
        assert_eq!(env("s-new"), accounts().env(&second), "the one picked");
        assert_eq!(env("s-old"), accounts().env(&first), "the first added");
        assert_eq!(spawned(&backend), 2, "no restart to switch accounts");
    }

    #[gpui_kit::test]
    fn each_tab_comes_back_in_its_permission_mode(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().expect("temp dir");
        let layouts = dir.path().join("layouts.json");
        let before = open_in(
            cx,
            Stored::default(),
            Some(layouts.clone()),
            no_accounts(),
            Some(env!("CARGO_MANIFEST_DIR").into()),
        );
        pick(cx, before, "s-new");
        click(cx, before, "chat-mode");
        click(cx, before, "chat-mode-Auto");
        click(cx, before, "chat-split-right");
        pick(cx, before, "s-old");

        let backend = Stored::default();
        open_in(
            cx,
            backend.clone(),
            Some(layouts),
            no_accounts(),
            Some(env!("CARGO_MANIFEST_DIR").into()),
        );
        let mode = |session: &str| {
            let specs = backend.0.borrow();
            let spec = specs
                .iter()
                .find(|spec| spec.resume.as_deref() == Some(session))
                .expect("resumed");
            spec.mode
        };
        assert_eq!(mode("s-new"), Some(PermissionMode::Auto), "the one picked");
        assert_eq!(mode("s-old"), None, "the user's own default");
    }
}
