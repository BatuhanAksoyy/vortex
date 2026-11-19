use std::path::{Path, PathBuf};

const APP_DIR: &str = "kiln";

/// `~/.config/kiln` on Linux and macOS (dotfile-friendly, like most developer
/// tools), the platform config directory on Windows.
#[must_use]
pub fn user_config_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        return directories::BaseDirs::new().map(|dirs| dirs.config_dir().join(APP_DIR));
    }
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(xdg).join(APP_DIR));
    }
    directories::BaseDirs::new().map(|dirs| dirs.home_dir().join(".config").join(APP_DIR))
}

#[must_use]
pub fn user_settings_path() -> Option<PathBuf> {
    user_config_dir().map(|dir| dir.join("settings.json"))
}

#[must_use]
pub fn user_keybindings_path() -> Option<PathBuf> {
    user_config_dir().map(|dir| dir.join("keybindings.json"))
}

/// Project layers in precedence order: shared, then personal (gitignored).
#[must_use]
pub fn project_settings_paths(project_root: &Path) -> [PathBuf; 2] {
    let dir = project_root.join(".kiln");
    [dir.join("settings.json"), dir.join("local.json")]
}
