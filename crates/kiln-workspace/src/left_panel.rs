//! How wide the activity bar is and whether it shows, kept across restarts.

use std::path::Path;

use anyhow::Context as _;
use serde::{Deserialize, Serialize};

pub const DEFAULT_WIDTH: f32 = 48.;
const MIN_WIDTH: f32 = 40.;
const MAX_WIDTH: f32 = 96.;

/// Widths are in pixels at the default zoom.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LeftPanel {
    width: f32,
    pub hidden: bool,
}

impl Default for LeftPanel {
    fn default() -> Self {
        Self {
            width: DEFAULT_WIDTH,
            hidden: false,
        }
    }
}

impl LeftPanel {
    pub fn width(self) -> f32 {
        self.width.clamp(MIN_WIDTH, MAX_WIDTH)
    }

    pub fn set_width(&mut self, width: f32) {
        self.width = width.clamp(MIN_WIDTH, MAX_WIDTH);
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
        let path = dir.path().join("data/left-panel.json");
        assert_eq!(LeftPanel::load(&path), LeftPanel::default());
        let mut panel = LeftPanel::default();
        panel.set_width(64.);
        panel.hidden = true;
        panel.save(&path).unwrap();
        assert_eq!(LeftPanel::load(&path), panel);
        panel.set_width(1000.);
        assert!((panel.width() - MAX_WIDTH).abs() < 0.1);
        std::fs::write(&path, "{").unwrap();
        assert_eq!(LeftPanel::load(&path), LeftPanel::default());
    }
}
