//! The places a setting is chosen, lowest first: every project (the user's
//! file), one project (its `.kiln` files) and one chat. Each goes over the
//! ones below it, and the defaults lie under them all.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::{
    edit::{get, set_in_file, set_in_value},
    load::{DEFAULT_SETTINGS, parse_jsonc, read_layer},
    merge::merge,
    paths::{project_settings_paths, user_settings_path},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scope {
    Global,
    Project,
    Chat,
}

/// What each scope sets, and the files the first two are kept in.
#[derive(Clone, Debug)]
pub struct Layers {
    defaults: Value,
    global: Value,
    project: Value,
    chat: Value,
    global_file: Option<PathBuf>,
    /// Where the project's shared settings are written.
    project_file: Option<PathBuf>,
    /// The project's personal file, which goes over the shared one.
    project_local: Option<PathBuf>,
}

impl Layers {
    /// The layers of the project at `project`, if any, and of a chat whose
    /// own settings are `chat`.
    ///
    /// # Errors
    /// When a settings file cannot be read or is not a JSON object.
    pub fn read(project: Option<&Path>, chat: Value) -> anyhow::Result<Self> {
        let (shared, local) = match project.map(project_settings_paths) {
            Some([shared, local]) => (Some(shared), Some(local)),
            None => (None, None),
        };
        let mut layers = Self {
            defaults: parse_jsonc(DEFAULT_SETTINGS)?,
            global: Value::Null,
            project: Value::Null,
            chat,
            global_file: user_settings_path(),
            project_file: shared,
            project_local: local,
        };
        layers.reread(Scope::Global)?;
        layers.reread(Scope::Project)?;
        Ok(layers)
    }

    /// Whether there is a project to set settings for.
    #[must_use]
    pub fn has_project(&self) -> bool {
        self.project_file.is_some()
    }

    /// What the chat sets.
    #[must_use]
    pub fn chat(&self) -> &Value {
        &self.chat
    }

    /// The value work at `scope` goes by at `key`, and the scope that set
    /// it; `None` for a default.
    #[must_use]
    pub fn value(&self, scope: Scope, key: &[&str]) -> (Option<&Value>, Option<Scope>) {
        for below in [Scope::Chat, Scope::Project, Scope::Global] {
            if below > scope {
                continue;
            }
            if let Some(value) = get(self.layer(below), key) {
                return (Some(value), Some(below));
            }
        }
        (get(&self.defaults, key), None)
    }

    /// Sets `key` at `scope`, or takes it out there with `None`; a file is
    /// written at once.
    ///
    /// # Errors
    /// When the scope's file cannot be read or written, or there is none.
    pub fn set(&mut self, scope: Scope, key: &[&str], value: Option<Value>) -> anyhow::Result<()> {
        let file = match scope {
            Scope::Chat => {
                set_in_value(&mut self.chat, key, value);
                return Ok(());
            }
            Scope::Global => self.global_file.as_deref(),
            Scope::Project => self.project_file.as_deref(),
        };
        let Some(file) = file else {
            anyhow::bail!("there is no settings file for {scope:?}");
        };
        set_in_file(file, key, value.as_ref())?;
        self.reread(scope)
    }

    fn layer(&self, scope: Scope) -> &Value {
        match scope {
            Scope::Global => &self.global,
            Scope::Project => &self.project,
            Scope::Chat => &self.chat,
        }
    }

    fn reread(&mut self, scope: Scope) -> anyhow::Result<()> {
        let read = |file: Option<&Path>| file.map_or(Ok(Value::Null), read_layer);
        match scope {
            Scope::Global => self.global = read(self.global_file.as_deref())?,
            Scope::Project => {
                let mut project = read(self.project_file.as_deref())?;
                merge(&mut project, read(self.project_local.as_deref())?);
                self.project = project;
            }
            Scope::Chat => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{Layers, Scope};

    const POLICY: [&str; 3] = ["git", "push", "policy"];

    fn at(layers: &Layers, scope: Scope) -> (Option<&serde_json::Value>, Option<Scope>) {
        layers.value(scope, &POLICY)
    }

    #[test]
    fn each_scope_goes_over_the_ones_below() {
        let dir = tempfile::tempdir().unwrap();
        let mut layers = Layers::read(Some(dir.path()), json!({})).unwrap();
        layers.global_file = Some(dir.path().join("user.json"));
        assert_eq!(
            layers.value(Scope::Chat, &POLICY),
            (Some(&json!("manual")), None)
        );

        layers
            .set(Scope::Global, &POLICY, Some(json!("never")))
            .unwrap();
        layers
            .set(Scope::Project, &POLICY, Some(json!("after_commit")))
            .unwrap();
        assert_eq!(
            at(&layers, Scope::Global),
            (Some(&json!("never")), Some(Scope::Global))
        );
        assert_eq!(
            at(&layers, Scope::Chat),
            (Some(&json!("after_commit")), Some(Scope::Project))
        );

        layers
            .set(Scope::Chat, &POLICY, Some(json!("session_end")))
            .unwrap();
        assert_eq!(at(&layers, Scope::Chat).1, Some(Scope::Chat));
        assert_eq!(
            at(&layers, Scope::Project).1,
            Some(Scope::Project),
            "only the chat's"
        );

        layers.set(Scope::Project, &POLICY, None).unwrap();
        let shared = std::fs::read_to_string(dir.path().join(".kiln/settings.json")).unwrap();
        assert!(!shared.contains("after_commit"), "{shared}");
        assert_eq!(at(&layers, Scope::Project).1, Some(Scope::Global));
    }

    #[test]
    fn the_personal_project_file_goes_over_the_shared_one() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".kiln")).unwrap();
        let local = r#"{"git": {"commit": {"review": false}}}"#;
        std::fs::write(dir.path().join(".kiln/local.json"), local).unwrap();
        let layers = Layers::read(Some(dir.path()), json!({})).unwrap();
        let review = layers.value(Scope::Project, &["git", "commit", "review"]);
        assert_eq!(review, (Some(&json!(false)), Some(Scope::Project)));
    }
}
