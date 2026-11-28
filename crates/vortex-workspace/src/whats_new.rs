//! The dialog that tells, on the first start of a newer Vortex, what
//! changed since the one that ran before.

use gpui_kit::prelude::*;
use gpui_kit::{
    App, SharedString, TestSupportExt as _, Window,
    component::{
        WindowExt as _,
        button::{Button, ButtonVariants as _},
        dialog::DialogFooter,
        text::TextView,
    },
    div, px,
};

/// Wide enough for a changelog line to read as one.
const WIDTH: f32 = 560.;

/// Shows `notes`, the changelog's Markdown, over the window.
pub fn show(notes: String, window: &mut Window, cx: &mut App) {
    let title = SharedString::from(format!("What's New in Vortex {}", vortex_update::CURRENT));
    let notes = SharedString::from(notes);
    window.open_dialog(cx, move |dialog, _, _| {
        dialog
            .title(title.clone())
            .w(px(WIDTH))
            .child(
                div()
                    .id("whats-new")
                    .test_support()
                    .aria_label(notes.clone())
                    .child(TextView::markdown("whats-new-notes", notes.clone()).selectable(true)),
            )
            .footer(
                DialogFooter::new().child(
                    Button::new("whats-new-close")
                        .primary()
                        .label("Close")
                        .on_click(|_, window, cx| window.close_dialog(cx)),
                ),
            )
    });
}
