//! Layered JSONC settings: embedded defaults, then the user file, then project files.

mod load;
mod merge;
mod model;
mod paths;
mod watch;

pub use load::{DEFAULT_SETTINGS, Diagnostic, LoadedSettings, SettingsSources, load};
pub use model::{EditorSettings, FilesSettings, Settings, ThemeMode, ThemeSettings, UiSettings};
pub use paths::{
    project_settings_paths, user_config_dir, user_keybindings_path, user_settings_path,
};
pub use watch::FileWatcher;
