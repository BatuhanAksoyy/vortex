//! The hooks button in the composer strip and the panel it opens: the hooks
//! by event, adding or changing one through the form, and removing one kept
//! in a file.

// Inside a `cfg(test)` module so helpers may `expect`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod hooks {
    use gpui_kit::TestAppContext;
    use gpui_kit::test::{TestQueryExt, TestWindowExt};
    use serde_json::json;
    use vortex_agent::{
        AgentEvent, Hook, HookEdit, HookEvent, HookScope, Hooks, SessionCommand, StoredHook,
    };
    use vortex_chat::Status;

    use super::common::Harness;

    fn stored(scope: HookScope, matcher: &str, command: &str) -> StoredHook {
        StoredHook {
            scope,
            matcher: matcher.into(),
            config: json!({ "type": "command", "command": command }),
        }
    }

    fn guard() -> Hook {
        Hook {
            event: "PreToolUse".into(),
            matcher: "Bash".into(),
            source: "Project settings".into(),
            text: "guard.sh".into(),
            kind: "command".into(),
            disabled: false,
            stored: Some(stored(HookScope::SharedProject, "Bash", "guard.sh")),
        }
    }

    fn plugin() -> Hook {
        Hook {
            event: "Stop".into(),
            matcher: String::new(),
            source: "Plugin".into(),
            text: "notify".into(),
            stored: None,
            ..guard()
        }
    }

    fn listing(hooks: Vec<Hook>) -> AgentEvent {
        let event = |name: &str, takes_matcher| HookEvent {
            name: name.into(),
            summary: format!("{name} for tests"),
            takes_matcher,
        };
        AgentEvent::Hooks(Hooks {
            hooks,
            events: vec![event("PreToolUse", true), event("Stop", false)],
            locked: None,
        })
    }

    /// A chat with the hooks panel open, before its first listing.
    fn opening(cx: &mut TestAppContext) -> Harness {
        let harness = Harness::new(cx);
        harness.commands();
        click(&harness, cx, "chat-hooks");
        assert_eq!(harness.commands(), [SessionCommand::RefreshHooks]);
        assert!(shown(&harness, cx, "chat-hooks-loading"));
        harness
    }

    fn opened(cx: &mut TestAppContext) -> Harness {
        let harness = opening(cx);
        harness.emit(cx, listing(vec![guard(), plugin()]));
        harness
    }

    fn shown(harness: &Harness, cx: &mut TestAppContext, id: &'static str) -> bool {
        harness.with_window(cx, |window, _| window.try_find(id).is_some())
    }

    fn click(harness: &Harness, cx: &mut TestAppContext, id: &'static str) {
        harness.with_window(cx, |window, cx| window.click(id, cx));
    }

    fn type_text(harness: &Harness, cx: &mut TestAppContext, text: &str) {
        harness.with_window(cx, |window, cx| window.input(text, cx));
    }

    const GUARD_REMOVE: &str = "hook-remove-PreToolUse-Bash-Project settings-guard.sh";
    const GUARD_EDIT: &str = "hook-edit-PreToolUse-Bash-Project settings-guard.sh";

    #[gpui_kit::test]
    fn the_button_lists_the_hooks_by_event(cx: &mut TestAppContext) {
        let harness = opened(cx);
        assert!(!shown(&harness, cx, "chat-hooks-loading"));
        harness.with_window(cx, |window, _| {
            let before = window.find_by_label("PreToolUse").bounds();
            let after = window.find_by_label("Stop").bounds();
            assert!(before.origin.y < after.origin.y);
            assert!(
                window
                    .find_by_label("Bash · Shared project settings")
                    .visible()
            );
        });
        assert!(shown(&harness, cx, GUARD_REMOVE));
        assert!(
            !shown(&harness, cx, "hook-remove-Stop--Plugin-notify"),
            "a plugin's hook is not kept in a settings file"
        );
        assert!(shown(&harness, cx, "hook-Stop--Plugin-notify"));
        assert_eq!(harness.status(cx), Status::Idle, "no turn started");
    }

    #[gpui_kit::test]
    fn the_form_adds_a_command_hook(cx: &mut TestAppContext) {
        let harness = opened(cx);
        click(&harness, cx, "hook-add");
        assert!(shown(&harness, cx, "hook-matcher"), "PreToolUse takes one");
        click(&harness, cx, "hook-event");
        click(&harness, cx, "hook-event-Stop");
        assert!(!shown(&harness, cx, "hook-matcher"), "Stop takes none");
        click(&harness, cx, "hook-scope");
        click(&harness, cx, "hook-scope-User");
        click(&harness, cx, "hook-save");
        assert!(harness.commands().is_empty(), "no command yet");
        type_text(&harness, cx, "say done");
        click(&harness, cx, "hook-save");
        let added = HookEdit::Add {
            scope: HookScope::User,
            event: "Stop".into(),
            matcher: String::new(),
            command: "say done".into(),
        };
        assert_eq!(harness.commands(), [SessionCommand::EditHook(added)]);
        assert!(!shown(&harness, cx, "hook-form"));
        harness.with_window(cx, |window, _| {
            window.find_by_label("Saving to all projects…");
        });
        harness.emit(cx, AgentEvent::HookEdited { error: None });
        let mut kept = plugin();
        kept.text = "say done".into();
        kept.source = "User settings".into();
        kept.stored = Some(stored(HookScope::User, "", "say done"));
        harness.emit(cx, listing(vec![guard(), plugin(), kept]));
        assert!(shown(
            &harness,
            cx,
            "hook-remove-Stop--User settings-say done"
        ));
    }

    #[gpui_kit::test]
    fn the_form_changes_a_hook_in_its_own_settings_file(cx: &mut TestAppContext) {
        let harness = opened(cx);
        assert!(
            !shown(&harness, cx, "hook-edit-Stop--Plugin-notify"),
            "a plugin's hook is changed with its plugin"
        );
        click(&harness, cx, GUARD_EDIT);
        assert!(shown(&harness, cx, "hook-form"));
        click(&harness, cx, "hook-save");
        assert!(harness.commands().is_empty(), "nothing changed");
        assert!(!shown(&harness, cx, "hook-form"));

        click(&harness, cx, GUARD_EDIT);
        type_text(&harness, cx, " --strict");
        click(&harness, cx, "hook-save");
        let changed = HookEdit::Replace {
            event: "PreToolUse".into(),
            matcher: "Bash".into(),
            command: "guard.sh --strict".into(),
            target_event: "PreToolUse".into(),
            target: guard().stored.expect("stored"),
        };
        assert_eq!(harness.commands(), [SessionCommand::EditHook(changed)]);
        assert!(shown(
            &harness,
            cx,
            "hook-PreToolUse-Bash-Saving to shared project settings…-guard.sh --strict"
        ));
        assert!(!shown(&harness, cx, GUARD_REMOVE), "the old form is gone");
    }

    #[gpui_kit::test]
    fn a_removed_hook_leaves_the_panel(cx: &mut TestAppContext) {
        let harness = opened(cx);
        click(&harness, cx, GUARD_REMOVE);
        let removed = HookEdit::Remove {
            event: "PreToolUse".into(),
            stored: guard().stored.expect("stored"),
        };
        assert_eq!(harness.commands(), [SessionCommand::EditHook(removed)]);
        assert!(!shown(
            &harness,
            cx,
            "hook-PreToolUse-Bash-Project settings-guard.sh"
        ));
        assert!(
            shown(&harness, cx, "chat-hooks-panel"),
            "the panel stays open"
        );
    }

    #[gpui_kit::test]
    fn a_failed_edit_says_why_and_keeps_the_hook(cx: &mut TestAppContext) {
        let harness = opened(cx);
        click(&harness, cx, GUARD_REMOVE);
        harness.commands();
        harness.emit(
            cx,
            AgentEvent::HookEdited {
                error: Some("settings.json is not valid JSON".into()),
            },
        );
        harness.with_window(cx, |window, _| {
            window.find_by_label("The hook was not saved: settings.json is not valid JSON");
        });
        assert!(shown(&harness, cx, GUARD_REMOVE));
    }

    #[gpui_kit::test]
    fn the_hooks_command_opens_the_panel(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.commands();
        harness.type_and_send(cx, "/hooks");
        assert_eq!(harness.commands(), [SessionCommand::RefreshHooks]);
        assert!(shown(&harness, cx, "chat-hooks-panel"));
    }

    #[gpui_kit::test]
    fn no_hooks_says_what_they_are(cx: &mut TestAppContext) {
        let harness = opening(cx);
        harness.emit(cx, listing(Vec::new()));
        assert!(shown(&harness, cx, "chat-hooks-empty"));
    }

    #[gpui_kit::test]
    fn hooks_an_organization_manages_cannot_be_changed(cx: &mut TestAppContext) {
        let harness = opening(cx);
        harness.emit(
            cx,
            AgentEvent::Hooks(Hooks {
                hooks: vec![guard()],
                events: Vec::new(),
                locked: Some("Managed by your organization.".into()),
            }),
        );
        assert!(shown(&harness, cx, "chat-hooks-locked"));
        assert!(!shown(&harness, cx, "hook-add"));
        assert!(!shown(&harness, cx, GUARD_REMOVE));
    }

    #[gpui_kit::test]
    fn escape_closes_the_panel_without_stopping_the_turn(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.type_and_send(cx, "run the tests");
        harness.commands();
        click(&harness, cx, "chat-hooks");
        harness.commands();
        harness.with_window(cx, |window, cx| window.press("escape", cx));
        assert!(!harness.commands().contains(&SessionCommand::Interrupt));
        assert!(!shown(&harness, cx, "chat-hooks-panel"));
        assert_eq!(harness.status(cx), Status::Running);
    }
}
