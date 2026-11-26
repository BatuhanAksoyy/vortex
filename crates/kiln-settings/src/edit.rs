//! Changing one setting in place: in a settings file, keeping the user's
//! comments and layout around it, or in a chat's own layer. A key is the
//! path to it, such as `["git", "push", "policy"]`; `None` takes it out, so
//! the layers below it apply again.

use std::path::Path;

use anyhow::Context as _;
use jsonc_parser::{
    ParseOptions,
    cst::{CstInputValue, CstObject, CstRootNode},
};
use serde_json::Value;

/// The value `layer` holds at `key`.
#[must_use]
pub fn get<'a>(layer: &'a Value, key: &[&str]) -> Option<&'a Value> {
    key.iter().try_fold(layer, |value, name| value.get(name))
}

/// Sets or takes out `key` in `layer`; objects left empty go with it.
pub fn set_in_value(layer: &mut Value, key: &[&str], value: Option<Value>) {
    let Some((name, parents)) = key.split_last() else {
        return;
    };
    if !layer.is_object() {
        *layer = Value::Object(serde_json::Map::new());
    }
    let Some(map) = layer.as_object_mut() else {
        return;
    };
    let Some((first, rest)) = key.split_first().filter(|_| !parents.is_empty()) else {
        match value {
            Some(value) => drop(map.insert((*name).to_owned(), value)),
            None => drop(map.remove(*name)),
        }
        return;
    };
    let child = map
        .entry((*first).to_owned())
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    set_in_value(child, rest, value);
    if child.as_object().is_some_and(serde_json::Map::is_empty) {
        map.remove(*first);
    }
}

/// Sets or takes out `key` in the settings file at `path`, creating it when
/// it is missing.
///
/// # Errors
/// When the file cannot be read, parsed or written.
pub fn set_in_file(path: &Path, key: &[&str], value: Option<&Value>) -> anyhow::Result<()> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Err(error).with_context(|| format!("cannot read {}", path.display()));
        }
    };
    let text = if text.trim().is_empty() {
        "{\n}\n"
    } else {
        &text
    };
    let root = CstRootNode::parse(text, &ParseOptions::default())
        .map_err(|error| anyhow::anyhow!("{}: {error}", path.display()))?;
    let object = root.object_value_or_set();
    match value {
        Some(value) => set_in_object(&object, key, value),
        None => remove_from_object(&object, key),
    }
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)
            .with_context(|| format!("cannot create {}", folder.display()))?;
    }
    std::fs::write(path, root.to_string())
        .with_context(|| format!("cannot write {}", path.display()))
}

fn set_in_object(object: &CstObject, key: &[&str], value: &Value) {
    let Some((name, parents)) = key.split_last() else {
        return;
    };
    let parent = parents.iter().fold(object.clone(), |object, name| {
        object.object_value_or_set(name)
    });
    let input = input(value);
    match parent.get(name) {
        Some(prop) => prop.set_value(input),
        None => drop(parent.append(name, input)),
    }
}

fn remove_from_object(object: &CstObject, key: &[&str]) {
    let Some((first, rest)) = key.split_first() else {
        return;
    };
    let Some(prop) = object.get(first) else {
        return;
    };
    if rest.is_empty() {
        prop.remove();
        return;
    }
    let Some(child) = prop.object_value() else {
        return;
    };
    remove_from_object(&child, rest);
    if child.properties().is_empty() {
        prop.remove();
    }
}

fn input(value: &Value) -> CstInputValue {
    match value {
        Value::Null => CstInputValue::Null,
        Value::Bool(value) => CstInputValue::Bool(*value),
        Value::Number(number) => CstInputValue::Number(number.to_string()),
        Value::String(text) => CstInputValue::String(text.clone()),
        Value::Array(items) => CstInputValue::Array(items.iter().map(input).collect()),
        Value::Object(map) => CstInputValue::Object(
            map.iter()
                .map(|(name, value)| (name.clone(), input(value)))
                .collect(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{get, set_in_file, set_in_value};

    #[test]
    fn a_value_is_set_and_taken_out_with_its_empty_parents() {
        let mut layer = json!({"chat": {"sleep_after_minutes": 5}});
        set_in_value(&mut layer, &["git", "push", "policy"], Some(json!("never")));
        assert_eq!(
            get(&layer, &["git", "push", "policy"]),
            Some(&json!("never"))
        );
        set_in_value(&mut layer, &["git", "push", "policy"], None);
        assert_eq!(layer, json!({"chat": {"sleep_after_minutes": 5}}));
    }

    #[test]
    fn a_file_keeps_its_comments_as_a_value_changes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let text = "{\n  // mine\n  \"ui\": { \"font_size\": 15 }, // size\n}\n";
        std::fs::write(&path, text).unwrap();
        set_in_file(&path, &["git", "push", "policy"], Some(&json!("never"))).unwrap();
        set_in_file(&path, &["ui", "font_size"], Some(&json!(16))).unwrap();
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(
            written.contains("// mine") && written.contains("// size"),
            "{written}"
        );
        let read = crate::read_layer(&path).unwrap();
        assert_eq!(
            read,
            json!({"ui": {"font_size": 16}, "git": {"push": {"policy": "never"}}})
        );

        set_in_file(&path, &["git", "push", "policy"], None).unwrap();
        assert_eq!(
            crate::read_layer(&path).unwrap(),
            json!({"ui": {"font_size": 16}})
        );
    }

    #[test]
    fn a_missing_file_is_made() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".kiln/settings.json");
        set_in_file(&path, &["git", "commit", "review"], Some(&json!(false))).unwrap();
        let read = crate::read_layer(&path).unwrap();
        assert_eq!(read, json!({"git": {"commit": {"review": false}}}));
    }
}
