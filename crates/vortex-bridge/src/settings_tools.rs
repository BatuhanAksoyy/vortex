//! The tools that read and change Vortex's settings, as the user would on the
//! settings page.

use serde_json::{Value, json};

use crate::tools::{Request, tool};

/// What an agent asks of Vortex's settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingsCall {
    /// One setting by its dotted key, or every one the settings page shows.
    Get { key: Option<String> },
    /// `value` `None` takes the setting out of `scope`.
    Set {
        key: String,
        value: Option<Value>,
        scope: SettingsScope,
    },
}

/// Where a change is kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsScope {
    /// The user's settings file, for every project.
    User,
    /// The project's `.vortex/settings.json`.
    Project,
}

pub(crate) fn list() -> [Value; 2] {
    [
        tool(
            "settings_get",
            "Vortex's settings. With `key`, such as `editor.font_size`, that setting's value and where it is set. Without it, every setting of the settings page with its value, the values it takes and what it does.",
            &json!({"key": {"type": "string"}}),
            &[],
        ),
        tool(
            "settings_set",
            "Changes one of Vortex's settings at once, as the user would on the settings page: fonts, theme, editor, layout, auto save, chat, git, updates. `scope` \"user\" (the default) applies to every project, \"project\" only to this one. A null `value` takes the setting out of that scope, so the one below applies again. A value that does not fit is refused with the reason.",
            &json!({
                "key": {"type": "string", "description": "Dotted, e.g. layout.browser"},
                "value": {"description": "A string, number, boolean or list, as the setting takes"},
                "scope": {"type": "string", "enum": ["user", "project"]},
            }),
            &["key", "value"],
        ),
    ]
}

/// The settings call `name` makes, if it is one.
pub(crate) fn parse(name: &str, arguments: &Value) -> Option<Result<Request, String>> {
    let key = arguments["key"].as_str().map(str::to_owned);
    let call = match name {
        "settings_get" => Ok(SettingsCall::Get { key }),
        "settings_set" => set(key, arguments),
        _ => return None,
    };
    Some(call.map(Request::Settings))
}

fn set(key: Option<String>, arguments: &Value) -> Result<SettingsCall, String> {
    let key = key.ok_or_else(|| "`key` must be a string".to_owned())?;
    let scope = match arguments["scope"].as_str() {
        None | Some("user") => SettingsScope::User,
        Some("project") => SettingsScope::Project,
        Some(_) => return Err("`scope` must be user or project".to_owned()),
    };
    let value = Some(arguments["value"].clone()).filter(|value| !value.is_null());
    Ok(SettingsCall::Set { key, value, scope })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{SettingsCall, SettingsScope, parse};
    use crate::tools::Request;

    #[test]
    fn a_change_names_its_setting_and_scope() {
        let call = parse(
            "settings_set",
            &json!({"key": "layout.browser", "value": "bottom", "scope": "project"}),
        );
        let expected = SettingsCall::Set {
            key: "layout.browser".into(),
            value: Some(json!("bottom")),
            scope: SettingsScope::Project,
        };
        assert_eq!(call, Some(Ok(Request::Settings(expected))));
        let reset = parse(
            "settings_set",
            &json!({"key": "ui.font_size", "value": null}),
        );
        let Some(Ok(Request::Settings(SettingsCall::Set { value, scope, .. }))) = reset else {
            panic!("not a change");
        };
        assert_eq!((value, scope), (None, SettingsScope::User));
        let wrong = parse("settings_set", &json!({"value": 1}))
            .unwrap()
            .unwrap_err();
        assert!(wrong.contains("`key`"), "{wrong}");
        assert_eq!(parse("browser_click", &json!({})), None);
    }
}
