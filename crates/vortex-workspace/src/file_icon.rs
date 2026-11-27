//! The colored icon of a file or folder.

use gpui_kit::{App, Img, Styled as _, component::ActiveTheme as _, img};
use vortex_file_icons::{asset_path, file_icon, folder_icon};

/// The icon of the file called `name`, at text size.
pub fn file(name: &str, cx: &App) -> Img {
    picture(file_icon(name, !cx.theme().is_dark()))
}

/// The icon of the folder called `name`, at text size.
pub fn folder(name: &str, open: bool, cx: &App) -> Img {
    picture(folder_icon(name, open, !cx.theme().is_dark()))
}

fn picture(icon: &str) -> Img {
    img(asset_path(icon)).flex_none().size_4()
}
