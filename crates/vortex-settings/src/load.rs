use std::path::{Path, PathBuf};

use anyhow::Context as _;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::{merge::merge, model::Settings};

pub const DEFAULT_SETTINGS: &str = include_str!("../assets/default-settings.jsonc");

/// Settings files applied over the defaults, lowest precedence first.
#[derive(Debug, Clone, Default)]
pub struct SettingsSources {
    pub layers: Vec<PathBuf>,
}

/// A problem found in one settings file. The file's other layers still apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct LoadedSettings {
    pub settings: Settings,
    /// The layers merged, which a chat's own settings go over.
    pub merged: Value,
    pub diagnostics: Vec<Diagnostic>,
}

/// Loads defaults and applies each layer in order. A layer that cannot be read,
/// parsed or validated is skipped and reported, so one bad file never takes the
/// application down or hides the other layers.
pub fn load(sources: &SettingsSources) -> anyhow::Result<LoadedSettings> {
    let mut merged = parse_jsonc(DEFAULT_SETTINGS).context("embedded default settings")?;
    let mut settings: Settings =
        serde_json::from_value(merged.clone()).context("embedded default settings")?;
    let mut diagnostics = Vec::new();

    for path in &sources.layers {
        match apply_layer(&merged, path) {
            Ok(None) => {}
            Ok(Some(layer)) => {
                diagnostics.extend(layer.unknown_keys.into_iter().map(|key| Diagnostic {
                    path: path.clone(),
                    message: format!("unknown setting `{key}`"),
                }));
                merged = layer.merged;
                settings = layer.settings;
            }
            Err(error) => diagnostics.push(Diagnostic {
                path: path.clone(),
                message: format!("{error:#}"),
            }),
        }
    }

    Ok(LoadedSettings {
        settings,
        merged,
        diagnostics,
    })
}

/// `merged` with a chat's own settings over it.
///
/// # Errors
/// When the chat's settings hold a value of the wrong type or out of range.
pub fn with_session(merged: &Value, session: &Value) -> anyhow::Result<Settings> {
    let mut merged = merged.clone();
    merge(&mut merged, session.clone());
    let settings: Settings = serde_json::from_value(merged).context("invalid setting value")?;
    let range_errors = settings.range_errors();
    if !range_errors.is_empty() {
        anyhow::bail!(range_errors.join("; "));
    }
    Ok(settings)
}

/// What the settings file at `path` holds; an empty object when it is
/// missing.
///
/// # Errors
/// When it cannot be read or is not a JSON object.
pub fn read_layer(path: &Path) -> anyhow::Result<Value> {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error).context("cannot read file"),
    };
    let layer = parse_jsonc(&content)?;
    if !layer.is_object() {
        anyhow::bail!("settings file must contain a JSON object");
    }
    Ok(layer)
}

struct AppliedLayer {
    merged: Value,
    settings: Settings,
    unknown_keys: Vec<String>,
}

fn apply_layer(base: &Value, path: &Path) -> anyhow::Result<Option<AppliedLayer>> {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("cannot read file"),
    };
    let overlay = parse_jsonc(&content)?;
    if !overlay.is_object() {
        anyhow::bail!("settings file must contain a JSON object");
    }
    let mut merged = base.clone();
    merge(&mut merged, overlay);
    let (settings, unknown_keys) = deserialize_tracking_unknown::<Settings>(merged.clone())?;
    let range_errors = settings.range_errors();
    if !range_errors.is_empty() {
        anyhow::bail!(range_errors.join("; "));
    }
    Ok(Some(AppliedLayer {
        merged,
        settings,
        unknown_keys,
    }))
}

pub(crate) fn parse_jsonc(content: &str) -> anyhow::Result<Value> {
    if content.trim().is_empty() {
        return Ok(Value::Object(serde_json::Map::new()));
    }
    serde_json_lenient::from_str(content).context("invalid JSON")
}

fn deserialize_tracking_unknown<T: DeserializeOwned>(
    value: Value,
) -> anyhow::Result<(T, Vec<String>)> {
    let mut unknown = Vec::new();
    let parsed = serde_ignored::deserialize(value, |path| unknown.push(path.to_string()))
        .context("invalid setting value")?;
    Ok((parsed, unknown))
}

#[cfg(test)]
mod tests {
    use super::{SettingsSources, load};
    use crate::ThemeMode;
    use std::path::PathBuf;

    fn write(dir: &tempfile::TempDir, name: &str, content: &str) -> PathBuf {
        let path = dir.path().join(name);
        std::fs::write(&path, content).unwrap();
        path
    }

    fn load_layers(layers: Vec<PathBuf>) -> super::LoadedSettings {
        load(&SettingsSources { layers }).unwrap()
    }

    #[test]
    fn defaults_load_without_layers() {
        let loaded = load_layers(vec![]);
        assert_eq!(loaded.diagnostics, [] as [super::Diagnostic; 0]);
        assert_eq!(loaded.settings.theme.mode, ThemeMode::System);
        assert_eq!(loaded.settings.theme.dark, "Vortex Dark");
    }

    #[test]
    fn later_layers_override_earlier_ones_and_accept_comments() {
        let dir = tempfile::tempdir().unwrap();
        let user = write(
            &dir,
            "user.json",
            r#"{ "ui": { "font_size": 16 }, // c
            "editor": { "tab_size": 2, }, }"#,
        );
        let project = write(&dir, "project.json", r#"{ "ui": { "font_size": 18 } }"#);
        let loaded = load_layers(vec![user, project]);
        assert_eq!(loaded.diagnostics, [] as [super::Diagnostic; 0]);
        assert!((loaded.settings.ui.font_size - 18.0).abs() < f32::EPSILON);
        assert_eq!(loaded.settings.editor.tab_size, 2);
    }

    #[test]
    fn missing_files_are_ignored_silently() {
        let loaded = load_layers(vec![PathBuf::from("/nonexistent/vortex/settings.json")]);
        assert_eq!(loaded.diagnostics, [] as [super::Diagnostic; 0]);
    }

    #[test]
    fn invalid_layer_is_skipped_and_reported() {
        let dir = tempfile::tempdir().unwrap();
        let broken = write(&dir, "broken.json", r#"{ "ui": { "font_size": "big" } }"#);
        let good = write(&dir, "good.json", r#"{ "editor": { "soft_wrap": true } }"#);
        let loaded = load_layers(vec![broken.clone(), good]);
        assert_eq!(loaded.diagnostics.len(), 1);
        assert_eq!(loaded.diagnostics[0].path, broken);
        assert!((loaded.settings.ui.font_size - 14.0).abs() < f32::EPSILON);
        assert!(loaded.settings.editor.soft_wrap);
    }

    #[test]
    fn out_of_range_values_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let layer = write(&dir, "s.json", r#"{ "editor": { "font_size": 500 } }"#);
        let loaded = load_layers(vec![layer]);
        assert_eq!(loaded.diagnostics.len(), 1);
        assert!(loaded.diagnostics[0].message.contains("editor.font_size"));
        assert!((loaded.settings.editor.font_size - 13.0).abs() < f32::EPSILON);
    }

    #[test]
    fn unknown_keys_are_reported_but_layer_applies() {
        let dir = tempfile::tempdir().unwrap();
        let layer = write(
            &dir,
            "s.json",
            r#"{ "ui": { "font_size": 15, "colour": 1 } }"#,
        );
        let loaded = load_layers(vec![layer]);
        assert_eq!(loaded.diagnostics.len(), 1);
        assert!(loaded.diagnostics[0].message.contains("ui.colour"));
        assert!((loaded.settings.ui.font_size - 15.0).abs() < f32::EPSILON);
    }

    #[test]
    fn empty_file_is_an_empty_layer() {
        let dir = tempfile::tempdir().unwrap();
        let layer = write(&dir, "s.json", "  \n");
        assert_eq!(
            load_layers(vec![layer]).diagnostics,
            [] as [super::Diagnostic; 0]
        );
    }
}
