//! Kiln's asset source: gpui-kit's default icons plus the few Kiln uses that
//! the default set leaves out.

use std::borrow::Cow;

use gpui_kit::{AssetSource, SharedString};

gpui_kit::assets::icon_assets!(ChatIcons, [Columns2, Lock, Pencil, Puzzle]);

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui_kit::Result<Option<Cow<'static, [u8]>>> {
        if let Some(bytes) = ChatIcons.load(path)? {
            return Ok(Some(bytes));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> gpui_kit::Result<Vec<SharedString>> {
        let mut paths = gpui_kit::assets::Assets.list(path)?;
        paths.extend(ChatIcons.list(path)?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_icons_and_default_icons_both_load() {
        for path in [
            "icons/pencil.svg",
            "icons/columns-2.svg",
            "icons/puzzle.svg",
            "icons/lock.svg",
            "icons/arrow-up.svg",
            "icons/plus.svg",
        ] {
            assert!(Assets.load(path).unwrap().is_some(), "{path}");
        }
    }
}
