//! The chat layout kept for each project, so Kiln reopens the tabs and
//! splits the user left. Only conversations are kept: a tab with nothing
//! said in it has nothing to resume.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::Context as _;
use kiln_agent::{Effort, PermissionMode};
use serde::{Deserialize, Serialize};

/// A group of tabs, or groups split side by side or stacked.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Pane {
    Split {
        vertical: bool,
        children: Vec<Slot>,
    },
    Tabs {
        active: usize,
        chats: Vec<SavedChat>,
    },
}

/// A tab: the conversation it resumes, and the added account, permission
/// mode and effort it runs with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedChat {
    pub session: String,
    /// `None` runs it with the usual sign-in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    /// `None` starts it in the mode the user's settings say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<PermissionMode>,
    /// `None` starts it with the effort the user's settings say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Slot {
    pub pane: Pane,
    /// The slot's width, or height in a vertical split, in pixels; `None`
    /// shares the room left.
    pub size: Option<f32>,
}

impl Pane {
    /// Without groups that hold no conversation and splits left with fewer
    /// than two slots; `None` when nothing is left.
    #[must_use]
    pub fn pruned(self) -> Option<Self> {
        match self {
            Self::Tabs { chats, .. } if chats.is_empty() => None,
            Self::Tabs { active, chats } => Some(Self::Tabs {
                active: active.min(chats.len() - 1),
                chats,
            }),
            Self::Split { vertical, children } => {
                let mut children: Vec<Slot> = children
                    .into_iter()
                    .filter_map(|slot| {
                        let pane = slot.pane.pruned()?;
                        Some(Slot { pane, ..slot })
                    })
                    .collect();
                match children.len() {
                    0 => None,
                    1 => children.pop().map(|slot| slot.pane),
                    _ => Some(Self::Split { vertical, children }),
                }
            }
        }
    }
}

/// The layouts of every project, kept in one file.
#[derive(Debug)]
pub struct LayoutFile {
    path: PathBuf,
}

impl LayoutFile {
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// The layout kept for `project`. A missing or damaged file keeps none:
    /// the worst case is one new chat, as on a first start.
    #[must_use]
    pub fn load(&self, project: &Path) -> Option<Pane> {
        self.read().remove(&key(project))
    }

    /// Keeps `pane` for `project`, or forgets the project's layout.
    pub fn save(&self, project: &Path, pane: Option<Pane>) -> anyhow::Result<()> {
        let mut layouts = self.read();
        let changed = match pane {
            Some(pane) => layouts.insert(key(project), pane.clone()) != Some(pane),
            None => layouts.remove(&key(project)).is_some(),
        };
        if !changed {
            return Ok(());
        }
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("cannot create {}", dir.display()))?;
        }
        // Renamed over the file, so a crash mid-write leaves the previous one.
        let temporary = self.path.with_extension("json.tmp");
        std::fs::write(&temporary, serde_json::to_string_pretty(&layouts)?)
            .with_context(|| format!("cannot write {}", temporary.display()))?;
        std::fs::rename(&temporary, &self.path)
            .with_context(|| format!("cannot write {}", self.path.display()))
    }

    fn read(&self) -> BTreeMap<String, Pane> {
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }
}

fn key(project: &Path) -> String {
    project.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tabs(active: usize, sessions: &[&str]) -> Pane {
        Pane::Tabs {
            active,
            chats: sessions
                .iter()
                .map(|id| SavedChat {
                    session: (*id).to_owned(),
                    account: id.strip_prefix("s-").map(|n| format!("account-{n}")),
                    mode: (*id == "s-1").then_some(PermissionMode::Auto),
                    effort: (*id == "s-2").then_some(Effort::Max),
                })
                .collect(),
        }
    }

    fn slot(pane: Pane, size: Option<f32>) -> Slot {
        Slot { pane, size }
    }

    #[test]
    fn layouts_are_kept_per_project() {
        let dir = tempfile::tempdir().unwrap();
        let file = LayoutFile::new(dir.path().join("layouts.json"));
        let split = Pane::Split {
            vertical: false,
            children: vec![
                slot(tabs(1, &["s-1", "s-2"]), Some(640.0)),
                slot(tabs(0, &["s-3"]), None),
            ],
        };
        file.save(Path::new("/a"), Some(split.clone())).unwrap();
        file.save(Path::new("/b"), Some(tabs(0, &["s-4"]))).unwrap();
        let again = LayoutFile::new(dir.path().join("layouts.json"));
        assert_eq!(again.load(Path::new("/a")), Some(split));
        assert_eq!(again.load(Path::new("/b")), Some(tabs(0, &["s-4"])));
        again.save(Path::new("/a"), None).unwrap();
        assert_eq!(again.load(Path::new("/a")), None);
        assert_eq!(again.load(Path::new("/b")), Some(tabs(0, &["s-4"])));
    }

    #[test]
    fn a_damaged_file_keeps_no_layout_and_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("layouts.json");
        std::fs::write(&path, "{").unwrap();
        let file = LayoutFile::new(path);
        assert_eq!(file.load(Path::new("/a")), None);
        file.save(Path::new("/a"), Some(tabs(0, &["s-1"]))).unwrap();
        assert_eq!(file.load(Path::new("/a")), Some(tabs(0, &["s-1"])));
    }

    #[test]
    fn groups_without_conversations_are_pruned() {
        let split = Pane::Split {
            vertical: true,
            children: vec![
                slot(tabs(0, &[]), Some(300.0)),
                slot(tabs(4, &["s-1", "s-2"]), Some(300.0)),
            ],
        };
        assert_eq!(split.pruned(), Some(tabs(1, &["s-1", "s-2"])));
        let empty = Pane::Split {
            vertical: false,
            children: vec![slot(tabs(0, &[]), None)],
        };
        assert_eq!(empty.pruned(), None);
    }
}
