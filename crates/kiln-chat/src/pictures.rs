//! Pictures as GPUI images: decoded once per picture and kept while the
//! picture lives, since a transcript redraws them on every frame.

use std::{
    cell::RefCell,
    sync::{Arc, Weak},
};

use gpui_kit::prelude::*;
use gpui_kit::{
    App, ElementId, Image, ImageFormat, ObjectFit, SharedString, TestSupportExt, WeakEntity,
    component::{ActiveTheme, h_flex},
    div, img, px,
};
use kiln_agent::Picture;

use crate::chat_view::ChatView;

/// How large a picture shows in the transcript; it keeps its proportions
/// within these bounds.
const MAX_WIDTH: f32 = 320.;
const MAX_HEIGHT: f32 = 200.;
/// A tiny picture, or one still loading, stays large enough to click.
const MIN_SIDE: f32 = 24.;

/// A picture's bytes, held weakly, and the image decoded from them.
type Decoded = (Weak<[u8]>, Arc<Image>);

thread_local! {
    static IMAGES: RefCell<Vec<Decoded>> = const { RefCell::new(Vec::new()) };
}

/// `None` for a type GPUI cannot draw.
pub fn image(picture: &Picture) -> Option<Arc<Image>> {
    let format = ImageFormat::from_mime_type(&picture.media_type)?;
    let data = Arc::downgrade(&picture.data);
    IMAGES.with_borrow_mut(|images| {
        if let Some((_, image)) = images.iter().find(|(held, _)| held.ptr_eq(&data)) {
            return Some(image.clone());
        }
        images.retain(|(held, _)| held.strong_count() > 0);
        let image = Arc::new(Image::from_bytes(format, picture.data.to_vec()));
        images.push((data, image.clone()));
        Some(image)
    })
}

/// Pictures in a row, wrapping, each within the transcript's bounds; a
/// click shows one at full size over `chat`.
pub fn gallery(
    id: impl Into<SharedString>,
    pictures: &[Picture],
    chat: &WeakEntity<ChatView>,
    cx: &App,
) -> impl IntoElement {
    let id = id.into();
    let theme = cx.theme();
    h_flex()
        .flex_wrap()
        .gap_2()
        .children(pictures.iter().enumerate().map(|(index, picture)| {
            let element_id = ElementId::Name(format!("{id}-{index}").into());
            let shown = image(picture).map_or_else(
                || {
                    div()
                        .px_2()
                        .py_1()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(picture.media_type.clone())
                        .into_any_element()
                },
                |image| {
                    img(image)
                        .max_w(px(MAX_WIDTH))
                        .max_h(px(MAX_HEIGHT))
                        .object_fit(ObjectFit::Contain)
                        .into_any_element()
                },
            );
            let chat = chat.clone();
            let clicked = picture.clone();
            div()
                .id(element_id)
                .test_support()
                .aria_label(SharedString::from(picture.media_type.clone()))
                .min_w(px(MIN_SIDE))
                .min_h(px(MIN_SIDE))
                .cursor_pointer()
                .on_click(move |_, window, cx| {
                    let picture = clicked.clone();
                    let _ = chat.update(cx, |chat, cx| chat.view_picture(picture, window, cx));
                })
                .overflow_hidden()
                .rounded(theme.radius)
                .border_1()
                .border_color(theme.border)
                .child(shown)
        }))
}
