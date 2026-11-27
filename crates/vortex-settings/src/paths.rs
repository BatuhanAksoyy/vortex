use std::path::{Path, PathBuf};

const APP_DIR: &str = "vortex";
/// Vortex's name before, whose folders are read while Vortex has none of
/// its own, so nothing set under the old name is lost.
const OLD_APP_DIR: &str = "kiln";

/// `name` in `base`, or `old` there when only that one exists.
#[must_use]
pub fn renamed_dir(base: &Path, name: &str, old: &str) -> PathBuf {
    let new = base.join(name);
    let old = base.join(old);
    if !new.exists() && old.exists() {
        old
    } else {
        new
    }
}

/// `~/.config/vortex` on Linux and macOS (dotfile-friendly, like most developer
/// tools), the platform config directory on Windows.
#[must_use]
pub fn user_config_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        return directories::BaseDirs::new()
            .map(|dirs| renamed_dir(dirs.config_dir(), APP_DIR, OLD_APP_DIR));
    }
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
        return Some(renamed_dir(Path::new(&xdg), APP_DIR, OLD_APP_DIR));
    }
    directories::BaseDirs::new()
        .map(|dirs| renamed_dir(&dirs.home_dir().join(".config"), APP_DIR, OLD_APP_DIR))
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
    let dir = renamed_dir(project_root, ".vortex", ".kiln");
    [dir.join("settings.json"), dir.join("local.json")]
}
