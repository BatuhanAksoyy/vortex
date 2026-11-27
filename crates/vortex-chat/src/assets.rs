//! Vortex's asset source: gpui-kit's default icons, the few Vortex uses that the
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
        CircleArrowUp,
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
        MessageSquare,
        Minimize,
        MousePointerClick,
        PanelLeft,
        PanelLeftClose,
        PanelLeftOpen,
        PanelsTopLeft,
        Paperclip,
        PenLine,
        Pencil,
        Puzzle,
        RefreshCw,
        Smartphone,
        SquareCode,
        Terminal,
        Usb,
        Trash
    ]
);

/// Vortex's mark, for the title bar: the simple one, as its cuts do not
/// hold at that size.
const BRAND: [(&str, &[u8]); 2] = [
    (
        "brand/mark-dark.svg",
        include_bytes!("../../../assets/brand/mark-dark-simple.svg"),
    ),
    (
        "brand/mark-light.svg",
        include_bytes!("../../../assets/brand/mark-light-simple.svg"),
    ),
];

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui_kit::Result<Option<Cow<'static, [u8]>>> {
        if let Some(svg) = vortex_file_icons::load(path) {
            return Ok(Some(Cow::Borrowed(svg.as_bytes())));
        }
        if let Some((_, bytes)) = BRAND.iter().find(|(name, _)| *name == path) {
            return Ok(Some(Cow::Borrowed(bytes)));
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
            "brand/mark-dark.svg",
            "brand/mark-light.svg",
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
