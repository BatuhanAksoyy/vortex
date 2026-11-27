//! The model the user last picked on each harness, which a new chat, a new
//! conversation and a chat handed over to that harness start with.

use std::{collections::BTreeMap, path::PathBuf};

use gpui_kit::{App, Global};

#[derive(Debug, Default)]
pub struct LastModels {
    /// Where they are kept; `None` keeps them in memory only, as in tests.
    file: Option<PathBuf>,
    /// By harness, as `AgentBackend::name` names it.
    models: BTreeMap<String, String>,
}

impl Global for LastModels {}

impl LastModels {
    /// The models kept in `file`; none when it is missing or unreadable.
    #[must_use]
    pub fn load(file: PathBuf) -> Self {
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        Self {
            models: serde_json::from_str(&text).unwrap_or_default(),
            file: Some(file),
        }
    }

    /// The model last picked on harness `agent`.
    pub fn of(agent: &str, cx: &App) -> Option<String> {
        cx.try_global::<Self>()?.models.get(agent).cloned()
    }

    /// Keeps `model` as the one last picked on `agent`.
    pub fn remember(agent: &str, model: &str, cx: &mut App) {
        let this = cx.default_global::<Self>();
        if this.models.get(agent).is_some_and(|kept| kept == model) {
            return;
        }
        this.models.insert(agent.to_owned(), model.to_owned());
        if let Some(file) = &this.file
            && let Err(error) = save(file, &this.models)
        {
            log::warn!("cannot keep the last model: {error:#}");
        }
    }
}

fn save(file: &std::path::Path, models: &BTreeMap<String, String>) -> anyhow::Result<()> {
    use anyhow::Context as _;
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder)
            .with_context(|| format!("cannot create {}", folder.display()))?;
    }
    std::fs::write(file, serde_json::to_string_pretty(models)?)
        .with_context(|| format!("cannot write {}", file.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn they_are_kept_by_harness() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("models.json");
        let mut models = BTreeMap::new();
        models.insert("codex".to_owned(), "gpt-6-luna".to_owned());
        save(&file, &models).unwrap();
        let loaded = LastModels::load(file);
        assert_eq!(loaded.models, models);
        let missing = LastModels::load(dir.path().join("none.json"));
        assert!(missing.models.is_empty());
    }
}
