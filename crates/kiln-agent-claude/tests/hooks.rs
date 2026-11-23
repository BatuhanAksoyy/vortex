//! Replays the captured `/hooks` listing: the rows Kiln shows, and the
//! edits that add, change and remove a hook, which the capture ran through
//! `claude edit-hook` and the next listing confirmed.

// Inside a `cfg(test)` module so helpers may `unwrap`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod hooks {
    use kiln_agent::{AgentEvent, HookEdit, HookScope, Hooks, SessionCommand, StoredHook};
    use kiln_agent_claude::Translator;
    use serde_json::{Value, json};

    use super::common::{fixture_path, output_lines};

    fn records() -> Vec<Value> {
        let recorded = std::fs::read_to_string(fixture_path("hooks")).unwrap();
        recorded
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    /// The two listings, before and after the edits.
    fn listings() -> (Translator, Vec<Hooks>) {
        let mut translator = Translator::default();
        for _ in 0..2 {
            let request = translator.on_command(SessionCommand::RefreshHooks).unwrap();
            let sent = records()
                .iter()
                .any(|record| record["dir"] == "in" && record["msg"] == request);
            assert!(sent, "{request} was not recorded");
        }
        let listed = output_lines("hooks")
            .iter()
            .flat_map(|line| translator.on_line(line))
            .filter_map(|event| match event {
                AgentEvent::Hooks(hooks) => Some(hooks),
                _ => None,
            })
            .collect();
        (translator, listed)
    }

    /// The edits the capture ran, each with whether the CLI took it.
    fn edits_run() -> Vec<(Value, bool)> {
        records()
            .into_iter()
            .filter(|record| record["dir"] == "note")
            .map(|record| (record["msg"]["edit"].clone(), record["msg"]["status"] == 0))
            .collect()
    }

    #[test]
    fn the_listing_shows_each_hook_and_where_it_is_kept() {
        let (_, listed) = listings();
        let [before, after] = listed.as_slice() else {
            panic!("expected two listings, got {}", listed.len());
        };
        let hook = &before.hooks[0];
        assert_eq!(
            (hook.event.as_str(), hook.matcher.as_str()),
            ("PreToolUse", "Bash")
        );
        assert_eq!(hook.text, "echo kiln-pre");
        assert_eq!(hook.kind, "command");
        assert_eq!(hook.source, "Local settings (.claude/settings.local.json)");
        assert_eq!(hook.stored.as_ref().unwrap().scope, HookScope::Project);
        assert!(!hook.disabled);
        assert_eq!(before.locked, None);
        assert_eq!(after.hooks.len(), 1);
        assert_eq!(after.hooks[0].text, "echo kiln-edited");
        assert_eq!(after.hooks[0].matcher, "Edit|Write");
        let stop = before
            .events
            .iter()
            .find(|event| event.name == "Stop")
            .unwrap();
        assert!(!stop.takes_matcher);
        assert!(
            before
                .events
                .iter()
                .any(|event| event.name == "PreToolUse" && event.takes_matcher)
        );
    }

    #[test]
    fn edits_are_the_ones_the_cli_took() {
        let (translator, listed) = listings();
        let listed_hook = listed[0].hooks[0].clone();
        let stop = StoredHook {
            scope: HookScope::Project,
            matcher: String::new(),
            config: json!({ "type": "command", "command": "echo kiln-stop" }),
        };
        let ours = [
            HookEdit::Add {
                scope: HookScope::Project,
                event: "Stop".into(),
                matcher: String::new(),
                command: "echo kiln-stop".into(),
            },
            HookEdit::Replace {
                event: "PreToolUse".into(),
                matcher: "Edit|Write".into(),
                command: "echo kiln-edited".into(),
                target_event: listed_hook.event,
                target: listed_hook.stored.unwrap(),
            },
            HookEdit::Remove {
                event: "Stop".into(),
                stored: stop,
            },
        ];
        let sent: Vec<(Value, bool)> = ours
            .iter()
            .map(|edit| (translator.hook_edit(edit), true))
            .collect();
        assert_eq!(sent, edits_run());
    }

    #[test]
    fn an_edit_is_no_message_to_the_session() {
        let mut translator = Translator::default();
        let edit = HookEdit::Add {
            scope: HookScope::User,
            event: "Stop".into(),
            matcher: String::new(),
            command: "true".into(),
        };
        assert_eq!(translator.on_command(SessionCommand::EditHook(edit)), None);
    }
}
