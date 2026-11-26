//! Files shown as what they are rather than as text: images, and Markdown
//! rendered beside its source.

use std::path::{Path, PathBuf};

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, ImageSource, ObjectFit, SharedString, SharedUri, TestSupportExt,
    component::{ActiveTheme as _, text::TextView, v_flex},
    div, img,
};

/// The widest a rendered line runs, for reading.
const MARKDOWN_MAX_WIDTH_REMS: f32 = 48.;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Text,
    Markdown,
    /// Shown as a picture; never read or written as text.
    Image,
}

impl Kind {
    pub fn of(path: Option<&Path>) -> Self {
        let extension = path
            .and_then(Path::extension)
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase);
        match extension.as_deref() {
            Some(
                "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "ico" | "tif" | "tiff",
            ) => Self::Image,
            Some("md" | "markdown") => Self::Markdown,
            _ => Self::Text,
        }
    }
}

pub fn image(path: &Path, cx: &App) -> AnyElement {
    v_flex()
        .id("editor-image")
        .test_support()
        .size_full()
        .items_center()
        .justify_center()
        .p_4()
        .bg(cx.theme().muted)
        .child(
            img(path.to_path_buf())
                .max_w_full()
                .max_h_full()
                .object_fit(ObjectFit::ScaleDown)
                .with_fallback(|| cannot_show().into_any_element()),
        )
        .into_any_element()
}

fn cannot_show() -> impl IntoElement {
    div().child("This image cannot be shown.")
}

/// `text` rendered, with images found beside the file it came from.
pub fn markdown(text: SharedString, path: Option<&Path>) -> AnyElement {
    let folder = path.and_then(Path::parent).map(Path::to_path_buf);
    let view = TextView::markdown("editor-markdown-text", text)
        .selectable(true)
        .image_source(move |uri| image_source(uri, folder.as_deref()));
    div()
        .id("editor-markdown")
        .test_support()
        .size_full()
        .overflow_y_scroll()
        .child(
            div()
                .w_full()
                .max_w(gpui_kit::rems(MARKDOWN_MAX_WIDTH_REMS))
                .mx_auto()
                .px_6()
                .py_4()
                .child(view),
        )
        .into_any_element()
}

/// A web address as it is; a path relative to the Markdown file's folder.
fn image_source(uri: &SharedUri, folder: Option<&Path>) -> ImageSource {
    let text = uri.to_string();
    if text.contains("://") || text.starts_with("data:") {
        return ImageSource::from(uri.clone());
    }
    let path = PathBuf::from(text.strip_prefix("file://").unwrap_or(&text));
    match folder {
        Some(folder) if path.is_relative() => ImageSource::from(folder.join(path)),
        _ => ImageSource::from(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_are_told_apart_by_extension() {
        assert_eq!(Kind::of(Some(Path::new("logo.PNG"))), Kind::Image);
        assert_eq!(Kind::of(Some(Path::new("icon.svg"))), Kind::Image);
        assert_eq!(Kind::of(Some(Path::new("README.md"))), Kind::Markdown);
        assert_eq!(Kind::of(Some(Path::new("main.rs"))), Kind::Text);
        assert_eq!(Kind::of(None), Kind::Text);
    }
}
