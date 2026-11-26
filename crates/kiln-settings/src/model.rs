use serde::Deserialize;

/// Fully resolved settings. Every field is present because defaults come from
/// `assets/default-settings.jsonc`, so no value is duplicated in Rust code.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Settings {
    pub theme: ThemeSettings,
    pub ui: UiSettings,
    pub editor: EditorSettings,
    pub chat: ChatSettings,
    pub files: FilesSettings,
    pub git: GitSettings,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ThemeSettings {
    pub mode: ThemeMode,
    pub light: String,
    pub dark: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct UiSettings {
    pub font_size: f32,
    pub font_family: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct EditorSettings {
    pub font_size: f32,
    pub font_family: String,
    pub tab_size: u8,
    pub line_numbers: bool,
    pub soft_wrap: bool,
    pub indent_guides: bool,
    pub minimap: MinimapSettings,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct MinimapSettings {
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ChatSettings {
    pub sleep_after_minutes: u64,
}

impl ChatSettings {
    /// How long a chat may stay idle; `None` keeps it running.
    #[must_use]
    pub fn sleep_after(&self) -> Option<std::time::Duration> {
        (self.sleep_after_minutes > 0)
            .then(|| std::time::Duration::from_mins(self.sleep_after_minutes))
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct FilesSettings {
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct GitSettings {
    pub commit: CommitSettings,
    pub push: PushSettings,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PushSettings {
    pub policy: PushPolicy,
    pub protected: Vec<String>,
    pub remote: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PushPolicy {
    Never,
    Manual,
    AfterCommit,
    SessionEnd,
}

/// When an agent's work is committed, and the rules every commit message
/// is held to.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CommitSettings {
    /// Kiln commits a chat's work for it: the strip over the chat with its
    /// Commit button, and `trigger`'s `task_done`.
    pub enabled: bool,
    pub strip: Vec<String>,
    pub subject_max: usize,
    pub conventional: bool,
    pub instructions: Vec<String>,
    pub require: Vec<String>,
    pub forbid: Vec<String>,
    pub review: bool,
    pub trigger: CommitTrigger,
    pub writer: WriterSettings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitTrigger {
    Agent,
    TaskDone,
}

/// The agent and model that judge a turn and write its commit message.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct WriterSettings {
    pub agent: String,
    pub model: String,
}

const FONT_SIZE_RANGE: std::ops::RangeInclusive<f32> = 6.0..=72.0;
const TAB_SIZE_RANGE: std::ops::RangeInclusive<u8> = 1..=16;

impl Settings {
    /// Returns a message for each value outside its accepted range.
    pub(crate) fn range_errors(&self) -> Vec<String> {
        let mut errors = Vec::new();
        for (key, size) in [
            ("ui.font_size", self.ui.font_size),
            ("editor.font_size", self.editor.font_size),
        ] {
            if !FONT_SIZE_RANGE.contains(&size) {
                errors.push(format!(
                    "`{key}` must be between {} and {}, got {size}",
                    FONT_SIZE_RANGE.start(),
                    FONT_SIZE_RANGE.end()
                ));
            }
        }
        if !TAB_SIZE_RANGE.contains(&self.editor.tab_size) {
            errors.push(format!(
                "`editor.tab_size` must be between {} and {}, got {}",
                TAB_SIZE_RANGE.start(),
                TAB_SIZE_RANGE.end(),
                self.editor.tab_size
            ));
        }
        errors
    }
}
