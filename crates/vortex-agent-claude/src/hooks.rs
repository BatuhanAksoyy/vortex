//! The rows of the CLI's `/hooks` menu (`get_hooks_listing`) and the edit
//! that adds or removes one. The CLI writes the edit itself, through its
//! `edit-hook` subcommand, as the VS Code extension does.

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::{Value, json};
use vortex_agent::{Hook, HookEdit, HookEvent, HookScope, Hooks, StoredHook};

const LOCKED: &str = "Your organization's settings manage hooks, so they can't be changed here.";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Reply {
    hooks: Vec<WireHook>,
    event_catalog: Vec<WireEvent>,
    policy: HashMap<String, Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireHook {
    event: String,
    matcher: String,
    source: String,
    source_label: String,
    #[serde(rename = "type")]
    kind: String,
    command_text: String,
    #[serde(default)]
    disabled: bool,
    editable: Option<Editable>,
}

/// The entry as stored, which an edit names as its target.
#[derive(Deserialize)]
struct Editable {
    matcher: String,
    config: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireEvent {
    name: String,
    summary: String,
    supports_matcher: bool,
}

/// The policy flags under which the CLI refuses every edit, as the menu
/// locks its controls: all hooks off, managed hooks only, plugin hooks only,
/// and managed settings that could not be read.
const LOCKS: [&str; 4] = [
    "disabledByPolicy",
    "managedOnly",
    "pluginOnly",
    "policyUnreadable",
];

pub fn from_reply(response: Value) -> Option<Hooks> {
    let reply: Reply = serde_json::from_value(response)
        .map_err(|error| log::warn!("unreadable hooks reply: {error}"))
        .ok()?;
    let locked = LOCKS
        .iter()
        .any(|flag| reply.policy.get(*flag) == Some(&Value::Bool(true)));
    Some(Hooks {
        hooks: reply.hooks.into_iter().map(hook).collect(),
        events: reply
            .event_catalog
            .into_iter()
            .map(|event| HookEvent {
                name: event.name,
                summary: event.summary,
                takes_matcher: event.supports_matcher,
            })
            .collect(),
        locked: locked.then(|| LOCKED.to_owned()),
    })
}

fn hook(wire: WireHook) -> Hook {
    let scope = scope_of(&wire.source);
    Hook {
        stored: scope
            .zip(wire.editable)
            .map(|(scope, editable)| StoredHook {
                scope,
                matcher: editable.matcher,
                config: editable.config,
            }),
        event: wire.event,
        matcher: wire.matcher,
        source: wire.source_label,
        text: wire.command_text,
        kind: wire.kind,
        disabled: wire.disabled,
    }
}

fn scope_of(source: &str) -> Option<HookScope> {
    HookScope::ALL
        .into_iter()
        .find(|scope| source_name(*scope) == source)
}

fn source_name(scope: HookScope) -> &'static str {
    match scope {
        HookScope::Project => "localSettings",
        HookScope::SharedProject => "projectSettings",
        HookScope::User => "userSettings",
    }
}

/// The edit `edit-hook` reads from stdin. A new hook runs a command, the
/// kind the form offers; a replaced one keeps its other settings.
pub fn edit(edit: &HookEdit) -> Value {
    match edit {
        HookEdit::Add {
            scope,
            event,
            matcher,
            command,
        } => json!({
            "op": "add",
            "source": source_name(*scope),
            "event": event,
            "matcher": matcher,
            "hook": { "type": "command", "command": command },
        }),
        HookEdit::Replace {
            event,
            matcher,
            command,
            target_event,
            target,
        } => {
            let mut hook = target.config.clone();
            hook["command"] = json!(command);
            json!({
                "op": "replace",
                "source": source_name(target.scope),
                "event": event,
                "matcher": matcher,
                "hook": hook,
                "target": { "event": target_event, "matcher": target.matcher, "hook": target.config },
            })
        }
        HookEdit::Remove { event, stored } => json!({
            "op": "remove",
            "source": source_name(stored.scope),
            "event": event,
            "matcher": stored.matcher,
            "target": { "event": event, "matcher": stored.matcher, "hook": stored.config },
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_scope_names_its_settings_file() {
        for scope in HookScope::ALL {
            assert_eq!(scope_of(source_name(scope)), Some(scope));
        }
        assert_eq!(scope_of("pluginHook"), None);
    }

    #[test]
    fn a_locking_policy_locks_the_hooks() {
        let reply = |managed_only: bool| {
            json!({
                "hooks": [], "eventCatalog": [],
                "policy": { "disabledByPolicy": false, "managedOnly": managed_only,
                            "pluginOnly": false, "allDisabled": false, "policyHookCount": 0 },
            })
        };
        assert_eq!(from_reply(reply(false)).unwrap().locked, None);
        assert_eq!(
            from_reply(reply(true)).unwrap().locked.as_deref(),
            Some(LOCKED)
        );
    }
}
