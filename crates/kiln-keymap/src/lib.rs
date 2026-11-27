//! VSCode-compatible keybindings.
//!
//! The defaults and the user's `keybindings.json` use the VSCode format, so a
//! user's existing VSCode overrides work unchanged. Resolution follows VSCode:
//! user entries come after the defaults, and an entry whose command starts with
//! `-` removes matching earlier bindings.

mod edit;
mod keystroke;

use anyhow::Context as _;
use serde::Deserialize;

pub use edit::{changes, rebind, reset};
pub use keystroke::to_gpui_keystrokes;

pub const DEFAULT_KEYBINDINGS: &str = include_str!("../assets/default-keybindings.jsonc");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Mac,
    Linux,
    Windows,
}

impl Platform {
    #[must_use]
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Mac
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Linux
        }
    }
}

/// One resolved binding, with keystrokes already in GPUI syntax.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub keystrokes: String,
    pub command: String,
    pub when: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Keymap {
    pub bindings: Vec<Binding>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Entry {
    key: Option<String>,
    mac: Option<String>,
    linux: Option<String>,
    win: Option<String>,
    command: String,
    when: Option<String>,
}

impl Entry {
    fn key_for(&self, platform: Platform) -> Option<&str> {
        let specific = match platform {
            Platform::Mac => &self.mac,
            Platform::Linux => &self.linux,
            Platform::Windows => &self.win,
        };
        specific.as_deref().or(self.key.as_deref())
    }
}

/// Resolves the embedded defaults plus the optional user file content.
/// A broken user file is reported and ignored; the defaults always apply.
pub fn resolve(user: Option<&str>, platform: Platform) -> anyhow::Result<Keymap> {
    let defaults = parse(DEFAULT_KEYBINDINGS).context("embedded default keybindings")?;
    let mut keymap = Keymap::default();
    apply(&mut keymap, defaults, platform);
    if let Some(content) = user {
        match parse(content) {
            Ok(entries) => apply(&mut keymap, entries, platform),
            Err(error) => keymap
                .diagnostics
                .push(format!("keybindings.json: {error:#}")),
        }
    }
    Ok(keymap)
}

fn parse(content: &str) -> anyhow::Result<Vec<Entry>> {
    if content.trim().is_empty() {
        return Ok(Vec::new());
    }
    serde_json_lenient::from_str(content).context("invalid keybindings JSON")
}

fn apply(keymap: &mut Keymap, entries: Vec<Entry>, platform: Platform) {
    for entry in entries {
        let keystrokes = match entry.key_for(platform).map(to_gpui_keystrokes) {
            Some(Ok(keystrokes)) => Some(keystrokes),
            Some(Err(error)) => {
                keymap
                    .diagnostics
                    .push(format!("`{}`: {error}", entry.command));
                continue;
            }
            None => None,
        };
        if let Some(removed) = entry.command.strip_prefix('-') {
            remove(&mut keymap.bindings, removed, keystrokes.as_deref());
            continue;
        }
        let Some(keystrokes) = keystrokes else {
            keymap
                .diagnostics
                .push(format!("`{}`: missing `key`", entry.command));
            continue;
        };
        keymap.bindings.push(Binding {
            keystrokes,
            command: entry.command,
            when: entry.when,
        });
    }
}

fn remove(bindings: &mut Vec<Binding>, command: &str, keystrokes: Option<&str>) {
    bindings.retain(|binding| {
        let same_command = binding.command == command;
        let same_keys = keystrokes.is_none_or(|keys| binding.keystrokes == keys);
        !(same_command && same_keys)
    });
}

#[cfg(test)]
mod tests {
    use super::{Platform, resolve};

    fn find<'a>(keymap: &'a super::Keymap, command: &str) -> Vec<&'a str> {
        keymap
            .bindings
            .iter()
            .filter(|b| b.command == command)
            .map(|b| b.keystrokes.as_str())
            .collect()
    }

    #[test]
    fn defaults_resolve_per_platform() {
        let linux = resolve(None, Platform::Linux).unwrap();
        let mac = resolve(None, Platform::Mac).unwrap();
        assert_eq!(linux.diagnostics, [] as [String; 0]);
        assert_eq!(find(&linux, "workbench.action.quickOpen"), ["ctrl-p"]);
        assert_eq!(find(&mac, "workbench.action.quickOpen"), ["cmd-p"]);
    }

    #[test]
    fn user_entries_add_bindings_after_defaults() {
        let user = r#"[{ "key": "ctrl+alt+p", "command": "workbench.action.quickOpen" }]"#;
        let keymap = resolve(Some(user), Platform::Linux).unwrap();
        assert_eq!(
            find(&keymap, "workbench.action.quickOpen"),
            ["ctrl-p", "ctrl-alt-p"]
        );
    }

    #[test]
    fn minus_command_removes_matching_default() {
        let user = r#"[
            // VSCode style removal with comments and trailing commas
            { "key": "ctrl+p", "command": "-workbench.action.quickOpen" },
        ]"#;
        let keymap = resolve(Some(user), Platform::Linux).unwrap();
        assert_eq!(find(&keymap, "workbench.action.quickOpen"), [] as [&str; 0]);
    }

    #[test]
    fn minus_command_without_key_removes_all_bindings_of_command() {
        let user = r#"[{ "command": "-workbench.action.quickOpen" }]"#;
        let keymap = resolve(Some(user), Platform::Linux).unwrap();
        assert_eq!(find(&keymap, "workbench.action.quickOpen"), [] as [&str; 0]);
    }

    #[test]
    fn broken_user_file_keeps_defaults() {
        let keymap = resolve(Some("[ not json"), Platform::Linux).unwrap();
        assert_eq!(keymap.diagnostics.len(), 1);
        assert_eq!(find(&keymap, "workbench.action.quickOpen"), ["ctrl-p"]);
    }

    #[test]
    fn entry_without_key_is_reported() {
        let keymap = resolve(Some(r#"[{ "command": "x" }]"#), Platform::Linux).unwrap();
        assert_eq!(keymap.diagnostics.len(), 1);
    }
}
