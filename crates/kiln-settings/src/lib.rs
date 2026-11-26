//! Layered JSONC settings: embedded defaults, then the user file, then project files.

mod edit;
mod load;
mod merge;
mod model;
mod paths;
mod scopes;
mod watch;

pub use edit::{get, set_in_file, set_in_value};
pub use load::{
    DEFAULT_SETTINGS, Diagnostic, LoadedSettings, SettingsSources, load, read_layer, with_session,
};
pub use model::{
    ChatSettings, CommitSettings, CommitTrigger, EditorSettings, FilesSettings, GitSettings,
    MinimapSettings, PushPolicy, PushSettings, Settings, ThemeMode, ThemeSettings, UiSettings,
    WriterSettings,
};
pub use paths::{
    project_settings_paths, user_config_dir, user_keybindings_path, user_settings_path,
};
pub use scopes::{Layers, Scope};
pub use watch::FileWatcher;
