/// Converts a VSCode key expression (`ctrl+shift+p`, `ctrl+k ctrl+o`) to GPUI
/// syntax (`ctrl-shift-p`, `ctrl-k ctrl-o`).
pub fn to_gpui_keystrokes(vscode: &str) -> Result<String, String> {
    let chords: Vec<String> = vscode
        .split_whitespace()
        .map(convert_chord)
        .collect::<Result<_, _>>()?;
    if chords.is_empty() {
        return Err("empty key".to_owned());
    }
    Ok(chords.join(" "))
}

fn convert_chord(chord: &str) -> Result<String, String> {
    let lower = chord.to_ascii_lowercase();
    let (modifiers, key) = split_key(&lower).ok_or_else(|| format!("invalid key `{chord}`"))?;
    let key = convert_key(key);
    let mut parts = modifiers
        .split('+')
        .filter(|part| !part.is_empty())
        .map(convert_modifier)
        .collect::<Result<Vec<&str>, _>>()?;
    parts.push(&key);
    Ok(parts.join("-"))
}

/// Splits `ctrl+shift+p` into (`ctrl+shift`, `p`). A trailing `+` is the plus
/// key itself, as in `ctrl++`.
fn split_key(chord: &str) -> Option<(&str, &str)> {
    if chord == "+" {
        return Some(("", "+"));
    }
    if let Some(modifiers) = chord.strip_suffix("++") {
        return Some((modifiers, "+"));
    }
    match chord.rsplit_once('+') {
        Some((_, "")) => None,
        Some((modifiers, key)) => Some((modifiers, key)),
        None => Some(("", chord)),
    }
}

fn convert_modifier(modifier: &str) -> Result<&'static str, String> {
    match modifier {
        "ctrl" => Ok("ctrl"),
        "shift" => Ok("shift"),
        "alt" => Ok("alt"),
        "cmd" => Ok("cmd"),
        "meta" | "win" | "super" => Ok("super"),
        other => Err(format!("unknown modifier `{other}`")),
    }
}

fn convert_key(key: &str) -> String {
    match key {
        "esc" => "escape".to_owned(),
        "oem_plus" | "numpad_add" => "+".to_owned(),
        "oem_minus" | "numpad_subtract" => "-".to_owned(),
        other => other.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::to_gpui_keystrokes;

    #[test]
    fn converts_modifiers_and_chords() {
        assert_eq!(to_gpui_keystrokes("ctrl+shift+p").unwrap(), "ctrl-shift-p");
        assert_eq!(
            to_gpui_keystrokes("Ctrl+K Ctrl+O").unwrap(),
            "ctrl-k ctrl-o"
        );
        assert_eq!(to_gpui_keystrokes("cmd+,").unwrap(), "cmd-,");
        assert_eq!(to_gpui_keystrokes("meta+e").unwrap(), "super-e");
    }

    #[test]
    fn handles_plus_and_minus_keys() {
        assert_eq!(to_gpui_keystrokes("ctrl+=").unwrap(), "ctrl-=");
        assert_eq!(to_gpui_keystrokes("ctrl+-").unwrap(), "ctrl--");
        assert_eq!(to_gpui_keystrokes("ctrl++").unwrap(), "ctrl-+");
        assert_eq!(to_gpui_keystrokes("escape").unwrap(), "escape");
        assert_eq!(to_gpui_keystrokes("esc").unwrap(), "escape");
    }

    #[test]
    fn rejects_unknown_modifiers_and_empty_keys() {
        assert!(to_gpui_keystrokes("hyper+x").is_err());
        assert!(to_gpui_keystrokes("   ").is_err());
    }
}
