use serde_json::json;
use vortex_agent::StoredHook;

use super::*;

fn event(name: &str, takes_matcher: bool) -> HookEvent {
    HookEvent {
        name: name.into(),
        summary: String::new(),
        takes_matcher,
    }
}

fn hook(event: &str, command: &str, scope: Option<HookScope>) -> Hook {
    Hook {
        event: event.into(),
        matcher: String::new(),
        source: "Local settings".into(),
        text: command.into(),
        kind: "command".into(),
        disabled: false,
        stored: scope.map(|scope| StoredHook {
            scope,
            matcher: String::new(),
            config: json!({ "type": "command", "command": command }),
        }),
    }
}

fn hooks(hooks: Vec<Hook>) -> Hooks {
    Hooks {
        hooks,
        events: vec![event("PreToolUse", true), event("Stop", false)],
        locked: None,
    }
}

fn add_stop(command: &str) -> HookEdit {
    HookEdit::Add {
        scope: HookScope::Project,
        event: "Stop".into(),
        matcher: String::new(),
        command: command.into(),
    }
}

fn shown(list: &HookList) -> Vec<(String, Vec<String>)> {
    list.by_event()
        .into_iter()
        .map(|(event, hooks)| (event, hooks.into_iter().map(|hook| hook.text).collect()))
        .collect()
}

const NONE: [(String, Vec<String>); 0] = [];

fn loaded(listed: Vec<Hook>) -> HookList {
    let mut list = HookList::default();
    assert!(!list.apply(hooks(listed)));
    list
}

#[test]
fn hooks_are_grouped_by_event_in_the_order_events_happen() {
    let list = loaded(vec![
        hook("Stop", "say done", None),
        hook("PreToolUse", "check", None),
        hook("Stop", "notify", None),
    ]);
    assert_eq!(
        shown(&list),
        [
            ("PreToolUse".to_owned(), vec!["check".to_owned()]),
            (
                "Stop".to_owned(),
                vec!["say done".to_owned(), "notify".to_owned()]
            ),
        ]
    );
    assert!(!HookList::default().is_loaded());
}

#[test]
fn an_added_hook_shows_at_once_and_holds_until_the_agent_lists_it() {
    let mut list = loaded(Vec::new());
    assert!(list.begin(add_stop("say done")));
    assert!(!list.begin(add_stop("other")), "one edit at a time");
    list.saved(None);
    assert!(list.apply(hooks(Vec::new())), "read again");
    let rows = list.by_event();
    let adding = &rows[0].1[0];
    assert_eq!(adding.text, "say done");
    assert_eq!(adding.stored, None, "not removable yet");
    assert!(!list.apply(hooks(vec![hook(
        "Stop",
        "say done",
        Some(HookScope::Project)
    )])));
    assert!(!list.is_busy());
    assert_eq!(list.notice(), None);
    assert!(list.by_event()[0].1[0].stored.is_some());
}

#[test]
fn a_removed_hook_hides_at_once_and_stays_hidden_until_the_agent_drops_it() {
    let kept = hook("Stop", "say done", Some(HookScope::User));
    let mut list = loaded(vec![kept.clone()]);
    assert!(list.begin(HookEdit::Remove {
        event: "Stop".into(),
        stored: kept.stored.clone().unwrap(),
    }));
    assert_eq!(shown(&list), NONE);
    list.saved(None);
    assert!(list.apply(hooks(vec![kept])), "still listed: read again");
    assert_eq!(shown(&list), NONE);
    assert!(!list.apply(hooks(Vec::new())));
    assert!(!list.is_busy());
}

#[test]
fn a_failed_edit_shows_why_and_the_hooks_as_they_are() {
    let mut list = loaded(Vec::new());
    assert!(list.begin(add_stop("say done")));
    list.saved(Some("edit_hook: hook is not valid"));
    assert!(!list.is_busy());
    assert_eq!(shown(&list), NONE);
    assert_eq!(
        list.error(),
        Some("The hook was not saved: edit_hook: hook is not valid")
    );
    list.clear_messages();
    assert_eq!(list.error(), None);
}

#[test]
fn an_edit_the_agent_never_lists_ends_with_a_notice() {
    let mut list = loaded(Vec::new());
    assert!(list.begin(add_stop("say done")));
    list.saved(None);
    let mut reads = 0;
    while list.apply(hooks(Vec::new())) {
        reads += 1;
    }
    assert_eq!(reads, CHECKS - 1);
    assert!(!list.is_busy());
    assert_eq!(list.notice(), Some(NOT_PICKED_UP));
}

#[test]
fn locked_hooks_take_no_edit() {
    let mut list = HookList::default();
    list.apply(Hooks {
        locked: Some("managed".into()),
        ..hooks(Vec::new())
    });
    assert_eq!(list.locked(), Some("managed"));
    assert!(!list.begin(add_stop("say done")));
}

#[test]
fn a_changed_hook_shows_its_new_form_until_the_agent_lists_it() {
    let kept = hook("Stop", "say done", Some(HookScope::User));
    let mut list = loaded(vec![kept.clone()]);
    assert!(list.begin(HookEdit::Replace {
        event: "Stop".into(),
        matcher: String::new(),
        command: "say finished".into(),
        target_event: "Stop".into(),
        target: kept.stored.clone().unwrap(),
    }));
    assert_eq!(
        shown(&list),
        [("Stop".to_owned(), vec!["say finished".to_owned()])]
    );
    list.saved(None);
    assert!(list.apply(hooks(vec![kept])), "old form: read again");
    assert_eq!(
        shown(&list),
        [("Stop".to_owned(), vec!["say finished".to_owned()])]
    );
    assert!(!list.apply(hooks(vec![hook(
        "Stop",
        "say finished",
        Some(HookScope::User)
    )])));
    assert!(!list.is_busy());
}
