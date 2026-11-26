//! The chat layout kept for the window, so Kiln reopens the tabs and splits
//! the user left, as editors reopen the last window. A tab with nothing said
//! in it comes back blank in its folder.

use std::{
    collections::{BTreeMap, BTreeSet},
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

/// A tab: the conversation it resumes, the folder it works in, and the added account, permission
/// mode and effort it runs with.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedChat {
    /// `None` for a tab with no conversation yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    /// `None` in a layout kept for one project, whose folder it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<PathBuf>,
    /// The harness it runs on, by its settings name; `None` for a chat kept
    /// before Kiln offered another, which ran on Claude Code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// `None` runs it with the usual sign-in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    /// `None` starts it in the mode the user's settings say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<PermissionMode>,
    /// `None` starts it with the effort the user's settings say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,
    /// The settings chosen for the chat alone; `None` when there are none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<serde_json::Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Slot {
    pub pane: Pane,
    /// The slot's width, or height in a vertical split, in pixels; `None`
    /// shares the room left.
    pub size: Option<f32>,
}

impl Pane {
    /// The folders its tabs work in that still exist.
    #[must_use]
    pub fn folders(&self) -> BTreeSet<PathBuf> {
        match self {
            Self::Split { children, .. } => children
                .iter()
                .flat_map(|slot| slot.pane.folders())
                .collect(),
            Self::Tabs { chats, .. } => chats
                .iter()
                .filter_map(|chat| chat.folder.clone())
                .filter(|folder| folder.is_dir())
                .collect(),
        }
    }

    /// With every tab that names no folder working in `folder`.
    fn in_folder(self, folder: &Path) -> Self {
        match self {
            Self::Split { vertical, children } => Self::Split {
                vertical,
                children: children
                    .into_iter()
                    .map(|slot| Slot {
                        pane: slot.pane.in_folder(folder),
                        ..slot
                    })
                    .collect(),
            },
            Self::Tabs { active, mut chats } => {
                for chat in &mut chats {
                    chat.folder.get_or_insert_with(|| folder.to_owned());
                }
                Self::Tabs { active, chats }
            }
        }
    }

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

/// The key the window's layout is kept under. Files written while each
/// project had a window of its own have one layout per project folder.
const WINDOW: &str = "window";

#[derive(Debug)]
pub struct LayoutFile {
    path: PathBuf,
}

impl LayoutFile {
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// The layout kept for the window, or else the one kept for `project`
    /// when each had its own. A missing or damaged file keeps none.
    #[must_use]
    pub fn load(&self, project: Option<&Path>) -> Option<Pane> {
        let mut layouts = self.read();
        layouts.remove(WINDOW).or_else(|| {
            let project = project?;
            let pane = layouts.remove(&*project.to_string_lossy())?;
            Some(pane.in_folder(project))
        })
    }

    /// The folders with a layout of their own.
    pub fn projects(&self) -> Vec<PathBuf> {
        let keys = self.read().into_keys();
        keys.filter(|key| key != WINDOW)
            .map(PathBuf::from)
            .collect()
    }

    /// Keeps `pane`, or forgets the layout.
    pub fn save(&self, pane: Option<Pane>) -> anyhow::Result<()> {
        let layouts: BTreeMap<String, Pane> = pane
            .map(|pane| (WINDOW.to_owned(), pane))
            .into_iter()
            .collect();
        if self.read() == layouts {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn tabs(active: usize, sessions: &[&str]) -> Pane {
        Pane::Tabs {
            active,
            chats: sessions
                .iter()
                .map(|id| SavedChat {
                    session: Some((*id).to_owned()),
                    folder: None,
                    agent: None,
                    account: id.strip_prefix("s-").map(|n| format!("account-{n}")),
                    mode: (*id == "s-1").then_some(PermissionMode::Auto),
                    effort: (*id == "s-2").then_some(Effort::Max),
                    settings: None,
                })
                .collect(),
        }
    }

    fn slot(pane: Pane, size: Option<f32>) -> Slot {
        Slot { pane, size }
    }

    #[test]
    fn the_windows_layout_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("layouts.json");
        let file = LayoutFile::new(path.clone());
        let split = Pane::Split {
            vertical: false,
            children: vec![
                slot(tabs(1, &["s-1", "s-2"]).in_folder(dir.path()), Some(640.0)),
                slot(tabs(0, &["s-3"]).in_folder(Path::new("/gone")), None),
            ],
        };
        file.save(Some(split.clone())).unwrap();
        let again = LayoutFile::new(path).load(None).unwrap();
        assert_eq!(again, split);
        let folders: Vec<_> = again.folders().into_iter().collect();
        assert_eq!(folders, [dir.path()], "only those that exist");
        file.save(None).unwrap();
        assert_eq!(file.load(None), None);
    }

    #[test]
    fn a_layout_kept_for_one_project_opens_in_its_folder() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("layouts.json");
        let kept = BTreeMap::from([("/a", tabs(0, &["s-1"]))]);
        std::fs::write(&path, serde_json::to_string(&kept).unwrap()).unwrap();
        let file = LayoutFile::new(path);
        assert_eq!(file.projects(), [Path::new("/a")]);
        assert_eq!(file.load(None), None);
        assert_eq!(file.load(Some(Path::new("/b"))), None);
        let opened = file.load(Some(Path::new("/a")));
        assert_eq!(opened, Some(tabs(0, &["s-1"]).in_folder(Path::new("/a"))));
    }

    #[test]
    fn a_damaged_file_keeps_no_layout_and_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("layouts.json");
        std::fs::write(&path, "{").unwrap();
        let file = LayoutFile::new(path);
        assert_eq!(file.load(None), None);
        file.save(Some(tabs(0, &["s-1"]))).unwrap();
        assert_eq!(file.load(None), Some(tabs(0, &["s-1"])));
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
