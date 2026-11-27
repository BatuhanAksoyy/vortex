//! Layered JSONC settings: embedded defaults, then the user file, then project files.

mod catalog;
mod edit;
mod load;
mod merge;
mod model;
mod paths;
mod scopes;
mod watch;

pub use catalog::{CATALOG, Entry, Kind, Section, check, find};
pub use edit::{get, set_in_file, set_in_value};
pub use load::{
    DEFAULT_SETTINGS, Diagnostic, LoadedSettings, SettingsSources, load, read_layer, with_session,
};
pub use model::{
    ActivityPage, AgentSettings, AutoSave, BrowserPlace, ChatSettings, ChatToolbar, CommitSettings,
    CommitTrigger, EditorSettings, FilesSettings, GitSettings, GutterSettings, HarnessColors,
    LayoutSettings, MinimapSettings, PushPolicy, PushSettings, ScrollBeyond, Settings, Side,
    ThemeMode, ThemeSettings, TypingSettings, UiSettings, UpdateSettings, UsageMeter, Whitespace,
    WriterSettings, parse_color,
};
pub use paths::{
    project_settings_paths, user_config_dir, user_keybindings_path, user_settings_path,
};
pub use scopes::{Layers, Scope};
pub use watch::FileWatcher;
