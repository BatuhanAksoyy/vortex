//! Kiln's asset source: gpui-kit's default icons, the few Kiln uses that the
//! default set leaves out, and the colored file icons.

use std::borrow::Cow;

use gpui_kit::{AssetSource, SharedString};

gpui_kit::assets::icon_assets!(
    ChatIcons,
    [
        ArrowLeft,
        ArrowRight,
        Bluetooth,
        BookOpen,
        Cable,
        Camera,
        Columns2,
        CopyMinus,
        Download,
        ExternalLink,
        FileDiff,
        FilePlus,
        FishingHook,
        FolderOpen,
        FolderPlus,
        FolderTree,
        Gauge,
        GitBranch,
        GitCommitHorizontal,
        Globe,
        Image,
        Lock,
        Maximize,
        Minimize,
        MousePointerClick,
        PanelLeft,
        PanelLeftClose,
        PanelLeftOpen,
        Paperclip,
        Pencil,
        Puzzle,
        RefreshCw,
        Smartphone,
        Terminal,
        Usb,
        Trash
    ]
);

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui_kit::Result<Option<Cow<'static, [u8]>>> {
        if let Some(svg) = kiln_file_icons::load(path) {
            return Ok(Some(Cow::Borrowed(svg.as_bytes())));
        }
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
    fn every_kind_of_icon_loads() {
        for path in [
            "icons/pencil.svg",
            "icons/columns-2.svg",
            "icons/puzzle.svg",
            "icons/git-commit-horizontal.svg",
            "icons/lock.svg",
            "icons/fishing-hook.svg",
            "icons/trash.svg",
            "icons/folder-tree.svg",
            "icons/git-branch.svg",
            "icons/globe.svg",
            "icons/camera.svg",
            "icons/terminal.svg",
            "icons/mouse-pointer-click.svg",
            "icons/download.svg",
            "icons/external-link.svg",
            "icons/arrow-up.svg",
            "icons/plus.svg",
            "icons/copy-minus.svg",
            "icons/file-diff.svg",
            "file-icons/rust.svg",
        ] {
            assert!(Assets.load(path).unwrap().is_some(), "{path}");
        }
    }
}
