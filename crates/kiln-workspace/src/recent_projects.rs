//! The folders sessions were started in, most recent first, kept across
//! restarts.

use std::path::{Path, PathBuf};

use anyhow::Context as _;

const MAX_KEPT: usize = 30;

#[derive(Default)]
pub struct RecentProjects {
    folders: Vec<PathBuf>,
    /// Where they are kept; `None` keeps none.
    file: Option<PathBuf>,
}

impl RecentProjects {
    /// What was kept in `file`. A missing or damaged one starts with
    /// `fallback`.
    pub fn load(file: PathBuf, fallback: impl FnOnce() -> Vec<PathBuf>) -> Self {
        let text = std::fs::read_to_string(&file).ok();
        let kept = text.and_then(|text| serde_json::from_str(&text).ok());
        Self {
            folders: kept.unwrap_or_else(fallback),
            file: Some(file),
        }
    }

    /// The folders that still exist.
    pub fn folders(&self) -> Vec<PathBuf> {
        let existing = self.folders.iter().filter(|folder| folder.is_dir());
        existing.cloned().collect()
    }

    /// Puts `folder` first.
    pub fn add(&mut self, folder: &Path) -> anyhow::Result<()> {
        if self.folders.first().is_some_and(|first| first == folder) {
            return Ok(());
        }
        self.folders.retain(|kept| kept != folder);
        self.folders.insert(0, folder.to_owned());
        self.folders.truncate(MAX_KEPT);
        let Some(file) = &self.file else {
            return Ok(());
        };
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("cannot create {}", dir.display()))?;
        }
        std::fs::write(file, serde_json::to_string_pretty(&self.folders)?)
            .with_context(|| format!("cannot write {}", file.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_used_comes_first_and_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("data/projects.json");
        let [a, b] = ["a", "b"].map(|name| dir.path().join(name));
        std::fs::create_dir(&a).unwrap();
        std::fs::create_dir(&b).unwrap();
        let mut recent = RecentProjects::load(file.clone(), || vec![a.clone()]);
        recent.add(&b).unwrap();
        recent.add(&a).unwrap();
        let again = RecentProjects::load(file, Vec::new);
        assert_eq!(again.folders(), [a.clone(), b.clone()]);
        std::fs::remove_dir(&a).unwrap();
        assert_eq!(again.folders(), [b], "a folder that is gone is left out");
    }
}
