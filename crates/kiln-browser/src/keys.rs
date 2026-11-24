//! The keyboard, as the protocol takes it.

use serde_json::{Value, json};

use crate::input::Modifiers;

/// A key by the name GPUI gives it (`a`, `enter`, `pageup`), and the text it
/// types, if any.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Key {
    pub name: String,
    pub text: Option<String>,
    pub modifiers: Modifiers,
    pub repeated: bool,
}

/// The key's DOM name and Windows key code, by GPUI's name for it.
const NAMED: [(&str, &str, u32); 15] = [
    ("backspace", "Backspace", 8),
    ("tab", "Tab", 9),
    ("enter", "Enter", 13),
    ("escape", "Escape", 27),
    ("pageup", "PageUp", 33),
    ("pagedown", "PageDown", 34),
    ("end", "End", 35),
    ("home", "Home", 36),
    ("left", "ArrowLeft", 37),
    ("up", "ArrowUp", 38),
    ("right", "ArrowRight", 39),
    ("down", "ArrowDown", 40),
    ("insert", "Insert", 45),
    ("delete", "Delete", 46),
    ("space", " ", 32),
];

impl Key {
    pub(crate) fn down(&self) -> Value {
        let mut params = self.params();
        let text = self.typed();
        params["type"] = if text.is_some() {
            "keyDown"
        } else {
            "rawKeyDown"
        }
        .into();
        if let Some(text) = text {
            params["text"] = text.into();
        }
        params["autoRepeat"] = self.repeated.into();
        if let Some(command) = self.edit_command() {
            params["commands"] = json!([command]);
        }
        params
    }

    pub(crate) fn up(&self) -> Value {
        let mut params = self.params();
        params["type"] = "keyUp".into();
        params
    }

    fn params(&self) -> Value {
        let (key, code, number) = self.identity();
        json!({
            "key": key,
            "code": code,
            "windowsVirtualKeyCode": number,
            "modifiers": self.modifiers.bits(),
        })
    }

    fn identity(&self) -> (String, String, u32) {
        let named = NAMED.iter().find(|(name, ..)| *name == self.name);
        if let Some((name, key, number)) = named {
            let code = if *name == "space" { "Space" } else { key };
            return ((*key).to_owned(), code.to_owned(), *number);
        }
        if let Some(number) = self.function_key() {
            let name = format!("F{number}");
            return (name.clone(), name, 111 + number);
        }
        let shown = self.text.clone().unwrap_or_else(|| self.name.clone());
        let mut characters = self.name.chars();
        match (characters.next(), characters.next()) {
            (Some(letter), None) if letter.is_ascii_alphabetic() => {
                let upper = letter.to_ascii_uppercase();
                (shown, format!("Key{upper}"), u32::from(upper))
            }
            (Some(digit), None) if digit.is_ascii_digit() => {
                (shown, format!("Digit{digit}"), u32::from(digit))
            }
            _ => (shown, String::new(), 0),
        }
    }

    fn function_key(&self) -> Option<u32> {
        let number: u32 = self.name.strip_prefix('f')?.parse().ok()?;
        (1..=24).contains(&number).then_some(number)
    }

    fn typed(&self) -> Option<String> {
        if self.modifiers.has(Modifiers::CONTROL) || self.modifiers.has(Modifiers::META) {
            return None;
        }
        match self.name.as_str() {
            "enter" => Some("\r".to_owned()),
            "space" => Some(" ".to_owned()),
            name if NAMED.iter().any(|(named, ..)| *named == name) => None,
            _ => self.text.clone(),
        }
    }

    /// The edit macOS binds to the key, which a browser without a window is
    /// not told by the system.
    fn edit_command(&self) -> Option<String> {
        let held = self.modifiers;
        if !cfg!(target_os = "macos") || held.has(Modifiers::CONTROL) {
            return None;
        }
        let edits: &[(&str, &str)] = if held.has(Modifiers::META) {
            &COMMAND_EDITS
        } else if held.has(Modifiers::ALT) {
            &OPTION_EDITS
        } else {
            &PLAIN_EDITS
        };
        let (_, edit) = edits.iter().find(|(name, _)| *name == self.name)?;
        let shift = held.has(Modifiers::SHIFT);
        Some(match *edit {
            "undo" if shift => "redo".to_owned(),
            edit if shift && edit.starts_with("move") => format!("{edit}AndModifySelection"),
            edit => edit.to_owned(),
        })
    }
}

const COMMAND_EDITS: [(&str, &str); 7] = [
    ("a", "selectAll"),
    ("z", "undo"),
    ("backspace", "deleteToBeginningOfLine"),
    ("left", "moveToBeginningOfLine"),
    ("right", "moveToEndOfLine"),
    ("up", "moveToBeginningOfDocument"),
    ("down", "moveToEndOfDocument"),
];
const OPTION_EDITS: [(&str, &str); 4] = [
    ("backspace", "deleteWordBackward"),
    ("delete", "deleteWordForward"),
    ("left", "moveWordLeft"),
    ("right", "moveWordRight"),
];
const PLAIN_EDITS: [(&str, &str); 6] = [
    ("backspace", "deleteBackward"),
    ("delete", "deleteForward"),
    ("left", "moveLeft"),
    ("right", "moveRight"),
    ("up", "moveUp"),
    ("down", "moveDown"),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn key(name: &str, text: Option<&str>, modifiers: Modifiers) -> Value {
        Key {
            name: name.to_owned(),
            text: text.map(str::to_owned),
            modifiers,
            repeated: false,
        }
        .down()
    }

    #[test]
    fn letters_type_and_named_keys_act() {
        let none = Modifiers::default();
        let typed = key("ğ", Some("ğ"), none);
        assert_eq!(typed["text"], "ğ");
        let upper = key("a", Some("A"), Modifiers::SHIFT);
        assert_eq!(
            (&upper["key"], &upper["code"]),
            (&"A".into(), &"KeyA".into())
        );
        assert_eq!(upper["windowsVirtualKeyCode"], 65);
        assert_eq!(key("enter", Some("\n"), none)["text"], "\r");
        let tab = key("tab", Some("\t"), none);
        assert_eq!(tab["type"], "rawKeyDown");
        assert_eq!(key("f5", None, none)["windowsVirtualKeyCode"], 116);
        assert_eq!(key("f", Some("f"), none)["code"], "KeyF");
    }

    #[test]
    fn shortcuts_type_nothing() {
        let meta = Modifiers::META;
        let all = key("a", Some("a"), meta);
        assert!(all.get("text").is_none());
        if cfg!(target_os = "macos") {
            assert_eq!(all["commands"], json!(["selectAll"]));
            let select = key("left", None, Modifiers::SHIFT);
            assert_eq!(select["commands"], json!(["moveLeftAndModifySelection"]));
        }
    }
}
