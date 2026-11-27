//! Agents reading and changing Vortex's settings: what the settings page
//! shows, for the project of the chat that asks.

use std::{fmt::Write as _, path::Path};

use gpui_kit::{Context, Window};
use serde_json::Value;
use vortex_bridge::{Reply, SettingsCall, SettingsScope};
use vortex_settings::{CATALOG, Kind, Layers, Scope};

use super::Workspace;

impl Workspace {
    pub(super) fn settings_call(
        &mut self,
        call: SettingsCall,
        project: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Reply {
        let layers = match Layers::read(Some(project), Value::Null) {
            Ok(layers) => layers,
            Err(error) => return Reply::failed(format!("Cannot read the settings: {error:#}")),
        };
        match call {
            SettingsCall::Get { key: Some(key) } => describe_one(&layers, &key),
            SettingsCall::Get { key: None } => Reply::text(describe_all(&layers)),
            SettingsCall::Set { key, value, scope } => {
                let reply = set(layers, &key, value, scope);
                if !reply.error {
                    self.reload_config(window, cx);
                }
                reply
            }
        }
    }
}

fn set(mut layers: Layers, key: &str, value: Option<Value>, scope: SettingsScope) -> Reply {
    if let Some(value) = &value
        && let Err(error) = vortex_settings::check(key, value)
    {
        return Reply::failed(format!("{error:#}"));
    }
    let (scope, place) = match scope {
        SettingsScope::User => (Scope::Global, "every project"),
        SettingsScope::Project => (Scope::Project, "this project"),
    };
    let path: Vec<&str> = key.split('.').collect();
    if let Err(error) = layers.set(scope, &path, value.clone()) {
        return Reply::failed(format!("Cannot change `{key}`: {error:#}"));
    }
    if let Some(value) = value {
        return Reply::text(format!("Set `{key}` to {value} for {place}."));
    }
    let (now, _) = layers.value(Scope::Project, &path);
    let now = now.map_or_else(|| "nothing".to_owned(), Value::to_string);
    Reply::text(format!(
        "Took `{key}` out of the settings for {place}; it is {now} now."
    ))
}

fn describe_one(layers: &Layers, key: &str) -> Reply {
    let path: Vec<&str> = key.split('.').collect();
    match layers.value(Scope::Project, &path) {
        (Some(value), from) => Reply::text(format!("`{key}` is {value}, {}.", source(from))),
        (None, _) => Reply::failed(format!("There is no setting `{key}`.")),
    }
}

fn describe_all(layers: &Layers) -> String {
    let mut told = String::from(
        "Vortex's settings, as the settings page shows them. Others, such as commit rules and \
         excluded files, are in the default settings and can be set by their keys too.\n",
    );
    for entry in CATALOG {
        let (value, from) = layers.value(Scope::Project, &entry.path());
        let value = value.map_or_else(String::new, Value::to_string);
        let _ = write!(
            told,
            "\n- `{}` = {value} ({}): {}",
            entry.key,
            source(from),
            entry.title
        );
        if !entry.description.is_empty() {
            let _ = write!(told, ". {}", entry.description);
        }
        let _ = write!(told, " Takes {}.", takes(entry.kind));
    }
    told
}

fn source(from: Option<Scope>) -> &'static str {
    match from {
        None => "the default",
        Some(Scope::Global) => "set for every project",
        Some(Scope::Project) => "set for this project",
        Some(Scope::Chat) => "set for the chat",
    }
}

fn takes(kind: Kind) -> String {
    match kind {
        Kind::Toggle => "true or false".to_owned(),
        Kind::Number { min, max, .. } => format!("a number from {min} to {max}"),
        Kind::Text => "text".to_owned(),
        Kind::Theme => "a theme's name, such as \"Vortex Dark\"".to_owned(),
        Kind::Choice(options) => {
            let values: Vec<String> = options.iter().map(|(it, _)| format!("\"{it}\"")).collect();
            format!("one of {}", values.join(", "))
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// Only the project's scope, so the user's own file is never written.
    #[test]
    fn an_agent_changes_a_projects_setting_and_reads_it_back() {
        let project = tempfile::tempdir().unwrap();
        let read = || Layers::read(Some(project.path()), Value::Null).unwrap();
        let wrong = set(
            read(),
            "editor.font_size",
            Some(json!("big")),
            SettingsScope::Project,
        );
        assert!(wrong.error);
        assert!(
            !project.path().join(".vortex").exists(),
            "nothing is written"
        );
        let reply = set(
            read(),
            "editor.font_size",
            Some(json!(17)),
            SettingsScope::Project,
        );
        assert_eq!(
            reply,
            Reply::text("Set `editor.font_size` to 17 for this project.")
        );
        let read_back = describe_one(&read(), "editor.font_size");
        assert_eq!(
            read_back,
            Reply::text("`editor.font_size` is 17, set for this project.")
        );
        assert!(describe_all(&read()).contains("- `editor.font_size` = 17 (set for this project)"));
        let reset = set(read(), "editor.font_size", None, SettingsScope::Project);
        assert!(!reset.error, "{reset:?}");
        assert!(describe_one(&read(), "editor.nope").error);
    }
}
