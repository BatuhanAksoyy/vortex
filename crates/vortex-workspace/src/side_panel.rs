//! What the panels beside the chats show, which of their parts are put
//! away, and how wide they and the activity bar are, kept across restarts.

use std::path::Path;

use anyhow::Context as _;
use serde::{Deserialize, Serialize};

pub const DEFAULT_BAR: f32 = 48.;
const MIN_BAR: f32 = 40.;
pub const MAX_BAR: f32 = 96.;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Page {
    #[default]
    Files,
    Git,
    Search,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SidePanel {
    /// The activity bar's width, in pixels at the default zoom.
    bar_width: f32,
    /// The panel's width, in pixels; `None` until the user sizes it.
    pub page_width: Option<f32>,
    pub page: Page,
    pub open: bool,
    /// Whether the panel and the activity bar are put away, whatever `open`
    /// says.
    pub hidden: bool,
    pub browser_open: bool,
    /// The browser's width, in pixels; `None` until the user sizes it.
    pub browser_width: Option<f32>,
    /// Its height while it is under the chats.
    pub browser_height: Option<f32>,
    pub put_away: PutAway,
}

/// The parts of the pages the user put away.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PutAway {
    /// The Files page's tree, leaving the editor.
    pub tree: bool,
    /// The Git page's changes and history, leaving the diff.
    git_list: bool,
    /// The Git page's diff, leaving the changes and history.
    git_diff: bool,
}

impl Default for SidePanel {
    fn default() -> Self {
        Self {
            bar_width: DEFAULT_BAR,
            page_width: None,
            page: Page::default(),
            open: false,
            hidden: false,
            browser_open: false,
            browser_width: None,
            browser_height: None,
            put_away: PutAway::default(),
        }
    }
}

impl PutAway {
    /// Whether the Git page's list and its diff are put away. Never both,
    /// or the page would be empty, even from a file edited by hand.
    pub fn git(self) -> (bool, bool) {
        (self.git_list, self.git_diff && !self.git_list)
    }

    /// Puts the Git page's list away, or shows it again; the diff comes back
    /// if it was away too.
    pub fn toggle_git_list(&mut self) {
        let (list, diff) = self.git();
        self.git_list = !list;
        self.git_diff = diff && list;
    }

    /// Puts the Git page's diff away, or shows it again; the list comes back
    /// if it was away too.
    pub fn toggle_git_diff(&mut self) {
        let (list, diff) = self.git();
        self.git_diff = !diff;
        self.git_list = list && diff;
    }
}

impl SidePanel {
    pub fn bar_width(self) -> f32 {
        self.bar_width.clamp(MIN_BAR, MAX_BAR)
    }

    pub fn set_bar_width(&mut self, width: f32) {
        self.bar_width = width.clamp(MIN_BAR, MAX_BAR);
    }

    /// What was kept in the file at `path`; a missing or damaged one keeps
    /// the defaults.
    pub fn load(path: &Path) -> Self {
        let text = std::fs::read_to_string(path).ok();
        let kept = text.and_then(|text| serde_json::from_str(&text).ok());
        kept.unwrap_or_default()
    }

    pub fn save(self, path: &Path) -> anyhow::Result<()> {
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)
                .with_context(|| format!("cannot create {}", folder.display()))?;
        }
        std::fs::write(path, serde_json::to_string_pretty(&self)?)
            .with_context(|| format!("cannot write {}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_is_kept_and_stays_within_bounds() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data/side-panel.json");
        assert_eq!(SidePanel::load(&path), SidePanel::default());
        let mut panel = SidePanel::default();
        panel.set_bar_width(64.);
        panel.page = Page::Git;
        panel.page_width = Some(700.);
        panel.open = true;
        panel.browser_open = true;
        panel.put_away.toggle_git_diff();
        panel.save(&path).unwrap();
        assert_eq!(SidePanel::load(&path), panel);
        panel.set_bar_width(1000.);
        assert!((panel.bar_width() - MAX_BAR).abs() < 0.1);
        std::fs::write(&path, "{").unwrap();
        assert_eq!(SidePanel::load(&path), SidePanel::default());
    }

    #[test]
    fn the_git_page_keeps_one_side_shown() {
        let mut away = PutAway::default();
        away.toggle_git_diff();
        assert_eq!(away.git(), (false, true));
        away.toggle_git_list();
        assert_eq!(away.git(), (true, false), "the diff comes back");
        away.toggle_git_diff();
        assert_eq!(away.git(), (false, true), "the list comes back");
        away.toggle_git_diff();
        assert_eq!(away.git(), (false, false));

        let both = r#"{ "put_away": { "git_list": true, "git_diff": true } }"#;
        let panel: SidePanel = serde_json::from_str(both).unwrap();
        assert_eq!(panel.put_away.git(), (true, false), "a file edited by hand");
    }
}
