use serde_json::json;

use super::{CATALOG, Kind, check};
use crate::{edit::get, load::DEFAULT_SETTINGS, load::parse_jsonc};

#[test]
fn every_entry_has_a_default_of_its_kind() {
    let defaults = parse_jsonc(DEFAULT_SETTINGS).unwrap();
    for entry in CATALOG {
        let value = get(&defaults, &entry.path());
        let value = value.unwrap_or_else(|| panic!("{} has no default", entry.key));
        let fits = match entry.kind {
            Kind::Toggle => value.is_boolean(),
            Kind::Number { .. } => value.is_number(),
            Kind::Text | Kind::Theme => value.is_string(),
            Kind::Choice(options) => options.iter().any(|(it, _)| value == *it),
        };
        assert!(fits, "{}: {value}", entry.key);
        check(entry.key, value).unwrap();
    }
}

#[test]
fn a_change_is_checked_against_the_settings() {
    check("editor.font_size", &json!(16)).unwrap();
    check("layout.browser", &json!("bottom")).unwrap();
    check("git.commit.instructions", &json!(["Write in English"])).unwrap();
    let wrong = check("editor.font_size", &json!("big")).unwrap_err();
    assert!(wrong.to_string().contains("editor.font_size"), "{wrong}");
    assert!(check("editor.font_size", &json!(500)).is_err());
    assert!(check("layout.browser", &json!("top")).is_err());
    let unknown = check("editor.font", &json!(1)).unwrap_err();
    assert_eq!(unknown.to_string(), "there is no setting `editor.font`");
    assert!(check("editor..font_size", &json!(1)).is_err());
}
