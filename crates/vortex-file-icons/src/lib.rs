//! The colored file and folder icons of Material Icon Theme 5.40.0 (MIT,
//! see `assets/LICENSE`), by the names VSCode shows them for.
//! `assets/pack.py` makes the pack.

use std::{collections::HashMap, sync::LazyLock};

use serde::Deserialize;

/// Where the icons are found among the app's assets.
pub const ASSET_PREFIX: &str = "file-icons/";

#[derive(Default, Deserialize)]
struct Pack {
    dark: Tables,
    /// What to show differently on a light background.
    light: Tables,
    icons: HashMap<String, String>,
}

#[derive(Default, Deserialize)]
struct Tables {
    files: HashMap<String, String>,
    extensions: HashMap<String, String>,
    folders: HashMap<String, String>,
    open_folders: HashMap<String, String>,
}

/// An unreadable pack leaves every icon out; the tests read it.
static PACK: LazyLock<Pack> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../assets/material-icons.json")).unwrap_or_default()
});

/// The icon of a file called `name`.
#[must_use]
pub fn file_icon(name: &str, light: bool) -> &'static str {
    let name = name.to_lowercase();
    find(light, |tables| tables.file(&name)).unwrap_or("file")
}

/// The icon of a folder called `name`.
#[must_use]
pub fn folder_icon(name: &str, open: bool, light: bool) -> &'static str {
    let name = name.to_lowercase();
    if open {
        find(light, |tables| tables.open_folders.get(&name)).unwrap_or("folder-open")
    } else {
        find(light, |tables| tables.folders.get(&name)).unwrap_or("folder")
    }
}

/// The asset path of the icon called `icon`.
#[must_use]
pub fn asset_path(icon: &str) -> String {
    format!("{ASSET_PREFIX}{icon}.svg")
}

/// The picture at an asset path from [`asset_path`].
#[must_use]
pub fn load(path: &str) -> Option<&'static str> {
    let icon = path.strip_prefix(ASSET_PREFIX)?.strip_suffix(".svg")?;
    PACK.icons.get(icon).map(String::as_str)
}

fn find(
    light: bool,
    look: impl Fn(&'static Tables) -> Option<&'static String>,
) -> Option<&'static str> {
    let special = light.then(|| look(&PACK.light)).flatten();
    special.or_else(|| look(&PACK.dark)).map(String::as_str)
}

impl Tables {
    /// By the whole name, then by the longest extension known: `d.ts` before
    /// `ts`.
    fn file(&self, name: &str) -> Option<&String> {
        let mut extensions = name.match_indices('.').map(|(dot, _)| &name[dot + 1..]);
        let by_name = self.files.get(name);
        by_name.or_else(|| extensions.find_map(|known| self.extensions.get(known)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_are_told_by_name_then_by_longest_extension() {
        assert_eq!(file_icon("main.rs", false), "rust");
        assert_eq!(file_icon("App.TSX", false), "react_ts");
        assert_eq!(file_icon("lib.d.ts", false), "typescript-def");
        assert_eq!(file_icon("package.json", false), "nodejs");
        assert_eq!(file_icon("unheard.of-kind", false), "file");
    }

    #[test]
    fn folders_open_and_have_a_light_variant() {
        assert_eq!(folder_icon("src", false, false), "folder-src");
        assert_eq!(folder_icon("src", true, false), "folder-src-open");
        assert_eq!(folder_icon("whatever", true, false), "folder-open");
        assert_eq!(folder_icon(".idea", false, true), "folder-intellij_light");
        assert_eq!(file_icon("Cargo.toml", true), "toml_light");
    }

    #[test]
    fn every_icon_named_is_in_the_pack() {
        for tables in [&PACK.dark, &PACK.light] {
            let maps = [
                &tables.files,
                &tables.extensions,
                &tables.folders,
                &tables.open_folders,
            ];
            for icon in maps.into_iter().flat_map(HashMap::values) {
                assert!(load(&asset_path(icon)).is_some(), "{icon}");
            }
        }
        for icon in ["file", "folder", "folder-open"] {
            assert!(load(&asset_path(icon)).is_some(), "{icon}");
        }
    }
}
