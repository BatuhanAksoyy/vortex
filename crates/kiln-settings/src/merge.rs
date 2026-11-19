use serde_json::Value;

/// Deep-merges `overlay` into `base`. Objects merge key by key; every other
/// value, including arrays, replaces the base value.
pub(crate) fn merge(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Object(base_map), Value::Object(overlay_map)) => {
            for (key, value) in overlay_map {
                match base_map.get_mut(&key) {
                    Some(existing) => merge(existing, value),
                    None => {
                        base_map.insert(key, value);
                    }
                }
            }
        }
        (base, overlay) => *base = overlay,
    }
}

#[cfg(test)]
mod tests {
    use super::merge;
    use serde_json::json;

    #[test]
    fn nested_objects_merge_and_scalars_replace() {
        let mut base = json!({"ui": {"font_size": 14, "font_family": ""}, "x": 1});
        merge(&mut base, json!({"ui": {"font_size": 16}, "x": 2}));
        assert_eq!(
            base,
            json!({"ui": {"font_size": 16, "font_family": ""}, "x": 2})
        );
    }

    #[test]
    fn arrays_replace_instead_of_appending() {
        let mut base = json!({"files": {"exclude": [".git", "target"]}});
        merge(&mut base, json!({"files": {"exclude": ["dist"]}}));
        assert_eq!(base, json!({"files": {"exclude": ["dist"]}}));
    }

    #[test]
    fn new_keys_are_added() {
        let mut base = json!({"a": 1});
        merge(&mut base, json!({"b": {"c": true}}));
        assert_eq!(base, json!({"a": 1, "b": {"c": true}}));
    }
}
