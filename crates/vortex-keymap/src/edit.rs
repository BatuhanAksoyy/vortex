//! Changing one command's keys in the user's `keybindings.json`, keeping
//! the comments and the entries of other commands as they are.

use jsonc_parser::{
    ParseOptions,
    cst::{CstArray, CstInputValue, CstRootNode},
};

use crate::to_gpui_keystrokes;

/// The file with `command` bound to `keys` alone, in VSCode syntax such as
/// `cmd+shift+p`; with no keys, the command has none. A removal entry comes
/// first, so the defaults' keys for it no longer apply.
///
/// # Errors
/// When the file is no JSON array or the keys cannot be read.
pub fn rebind(user: &str, command: &str, keys: Option<&str>) -> anyhow::Result<String> {
    if let Some(keys) = keys {
        to_gpui_keystrokes(keys).map_err(|error| anyhow::anyhow!("`{keys}`: {error}"))?;
    }
    edit(user, |entries| {
        remove_entries(entries, command);
        entries.append(object(&[("command", &format!("-{command}"))]));
        if let Some(keys) = keys {
            entries.append(object(&[("key", keys), ("command", command)]));
        }
    })
}

/// The file without the user's entries for `command`, so its default keys
/// apply again.
///
/// # Errors
/// When the file is no JSON array.
pub fn reset(user: &str, command: &str) -> anyhow::Result<String> {
    edit(user, |entries| remove_entries(entries, command))
}

/// Whether the user's file changes `command`'s keys.
#[must_use]
pub fn changes(user: &str, command: &str) -> bool {
    let Ok(root) = CstRootNode::parse(user, &ParseOptions::default()) else {
        return false;
    };
    root.array_value()
        .is_some_and(|entries| !entries_for(&entries, command).is_empty())
}

fn edit(user: &str, change: impl FnOnce(&CstArray)) -> anyhow::Result<String> {
    let text = if user.trim().is_empty() {
        "[\n]\n"
    } else {
        user
    };
    let root = CstRootNode::parse(text, &ParseOptions::default())
        .map_err(|error| anyhow::anyhow!("keybindings.json: {error}"))?;
    let entries = root
        .array_value()
        .ok_or_else(|| anyhow::anyhow!("keybindings.json holds no list"))?;
    change(&entries);
    Ok(root.to_string())
}

fn remove_entries(entries: &CstArray, command: &str) {
    for entry in entries_for(entries, command) {
        entry.remove();
    }
}

/// The entries that bind or unbind `command`.
fn entries_for(entries: &CstArray, command: &str) -> Vec<jsonc_parser::cst::CstNode> {
    let removal = format!("-{command}");
    entries
        .elements()
        .into_iter()
        .filter(|entry| {
            let named = entry
                .as_object()
                .and_then(|object| object.get("command"))
                .and_then(|prop| prop.value())
                .and_then(|value| value.as_string_lit())
                .and_then(|text| text.decoded_value().ok());
            named.is_some_and(|named| named == command || named == removal)
        })
        .collect()
}

fn object(fields: &[(&str, &str)]) -> CstInputValue {
    CstInputValue::Object(
        fields
            .iter()
            .map(|(name, value)| {
                (
                    (*name).to_owned(),
                    CstInputValue::String((*value).to_owned()),
                )
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::{changes, rebind, reset};
    use crate::{Platform, resolve};

    fn keys(user: &str, command: &str) -> Vec<String> {
        let keymap = resolve(Some(user), Platform::Linux).unwrap();
        let bound = keymap.bindings.into_iter().filter(|b| b.command == command);
        bound.map(|binding| binding.keystrokes).collect()
    }

    const QUICK_OPEN: &str = "workbench.action.quickOpen";

    #[test]
    fn a_command_takes_new_keys_in_place_of_its_defaults() {
        let user = "[\n  // mine\n  { \"key\": \"ctrl+j\", \"command\": \"workbench.action.files.save\" }\n]\n";
        let user = rebind(user, QUICK_OPEN, Some("ctrl+alt+o")).unwrap();
        assert_eq!(keys(&user, QUICK_OPEN), ["ctrl-alt-o"]);
        assert!(user.contains("// mine"), "{user}");
        assert_eq!(
            keys(&user, "workbench.action.files.save"),
            ["ctrl-s", "ctrl-j"]
        );
        let user = rebind(&user, QUICK_OPEN, Some("ctrl+e")).unwrap();
        assert_eq!(keys(&user, QUICK_OPEN), ["ctrl-e"], "replaced, not added");
        assert!(changes(&user, QUICK_OPEN));
    }

    #[test]
    fn a_command_loses_its_keys_and_gets_its_defaults_back() {
        let user = rebind("", QUICK_OPEN, None).unwrap();
        assert_eq!(keys(&user, QUICK_OPEN), [] as [&str; 0]);
        let user = reset(&user, QUICK_OPEN).unwrap();
        assert_eq!(keys(&user, QUICK_OPEN), ["ctrl-p"]);
        assert!(!changes(&user, QUICK_OPEN));
    }

    #[test]
    fn unreadable_keys_and_files_are_errors() {
        assert!(rebind("", QUICK_OPEN, Some("hyper+p")).is_err());
        assert!(rebind("{}", QUICK_OPEN, Some("ctrl+p")).is_err());
        assert!(rebind("[ oops", QUICK_OPEN, Some("ctrl+p")).is_err());
    }
}
