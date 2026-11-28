//! A menu item's label that a long name cannot widen past the window.

use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, App, SharedString, TestSupportExt as _, Window, div, px};

/// Past it a name in a menu is cut short.
const LABEL_MAX_WIDTH: f32 = 320.;

/// A menu item's label, cut short with an ellipsis past
/// `LABEL_MAX_WIDTH`: a plain item's label never shrinks, and a long one
/// runs out of the menu.
/// Named by the whole label, which the menu's own items are too.
pub(crate) fn truncated(
    label: SharedString,
) -> impl Fn(&mut Window, &mut App) -> AnyElement + 'static {
    move |_, _| {
        div()
            .id(SharedString::from(format!("menu-label-{label}")))
            .test_support()
            .aria_label(label.clone())
            .max_w(px(LABEL_MAX_WIDTH))
            .truncate()
            .child(label.clone())
            .into_any_element()
    }
}
