//! What the panels beside the chats show, and how wide they and the activity
//! bar are, kept across restarts.

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
        }
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
        panel.save(&path).unwrap();
        assert_eq!(SidePanel::load(&path), panel);
        panel.set_bar_width(1000.);
        assert!((panel.bar_width() - MAX_BAR).abs() < 0.1);
        std::fs::write(&path, "{").unwrap();
        assert_eq!(SidePanel::load(&path), SidePanel::default());
    }
}
