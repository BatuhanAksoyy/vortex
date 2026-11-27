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
    pub layout: LayoutSettings,
    pub update: UpdateSettings,
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
    pub gutter: GutterSettings,
    pub typing: TypingSettings,
    pub show_whitespace: Whitespace,
    pub scroll_beyond_last_line: ScrollBeyond,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct GutterSettings {
    pub folds: bool,
}

/// What the editor does as the user types.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TypingSettings {
    pub auto_close_brackets: bool,
    pub auto_indent: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Whitespace {
    None,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScrollBeyond {
    HalfPage,
    Off,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct MinimapSettings {
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ChatSettings {
    pub sleep_after_minutes: u64,
    pub toolbar: ChatToolbar,
    pub usage_meter: UsageMeter,
    pub colors: HarnessColors,
}

/// The items of each place, by name; unknown names are left out.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ChatToolbar {
    pub top: Vec<String>,
    pub bottom_start: Vec<String>,
    pub bottom_end: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageMeter {
    Fuller,
    Session,
    Weekly,
    Both,
}

/// `#rrggbb`, or empty for the harness's own color.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct HarnessColors {
    pub claude: String,
    pub codex: String,
}

/// `color` as `0xrrggbb`, when it is a `#rrggbb` color.
#[must_use]
pub fn parse_color(color: &str) -> Option<u32> {
    let hex = color.strip_prefix('#').filter(|hex| hex.len() == 6)?;
    u32::from_str_radix(hex, 16).ok()
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
    pub auto_save: AutoSave,
    pub auto_save_delay: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoSave {
    Off,
    AfterDelay,
}

/// Where the window's parts sit, and which are shown.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LayoutSettings {
    pub side_panel: Side,
    pub browser: BrowserPlace,
    pub file_tree: Side,
    pub activity_bar: bool,
    pub status_bar: bool,
    pub title_bar_buttons: bool,
    pub activity_order: Vec<ActivityPage>,
}

impl LayoutSettings {
    /// The activity bar's pages in the user's order, each once; a page the
    /// order leaves out comes after the rest.
    #[must_use]
    pub fn activity_pages(&self) -> Vec<ActivityPage> {
        let mut pages: Vec<ActivityPage> = Vec::new();
        let all = [
            ActivityPage::Files,
            ActivityPage::Search,
            ActivityPage::Changes,
        ];
        for page in self.activity_order.iter().chain(&all) {
            if !pages.contains(page) {
                pages.push(*page);
            }
        }
        pages
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserPlace {
    Left,
    Right,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityPage {
    Files,
    Search,
    Changes,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct UpdateSettings {
    pub check: bool,
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
    /// Vortex commits a chat's work for it: the strip over the chat with its
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
const AUTO_SAVE_DELAY_RANGE: std::ops::RangeInclusive<u64> = 100..=60_000;

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
        if !AUTO_SAVE_DELAY_RANGE.contains(&self.files.auto_save_delay) {
            errors.push(format!(
                "`files.auto_save_delay` must be between {} and {}, got {}",
                AUTO_SAVE_DELAY_RANGE.start(),
                AUTO_SAVE_DELAY_RANGE.end(),
                self.files.auto_save_delay
            ));
        }
        for (key, color) in [
            ("chat.colors.claude", &self.chat.colors.claude),
            ("chat.colors.codex", &self.chat.colors.codex),
        ] {
            if !color.is_empty() && parse_color(color).is_none() {
                errors.push(format!(
                    "`{key}` must be a color like #e5484d, got {color:?}"
                ));
            }
        }
        errors
    }
}
