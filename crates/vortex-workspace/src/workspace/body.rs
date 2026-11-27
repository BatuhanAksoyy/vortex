//! The window's frame around the pages: the activity bar on its side of
//! the panels and chats, and the status bar.

use std::path::Path;

use gpui_kit::prelude::*;
use gpui_kit::{
    Context, Window,
    component::{h_flex, status_bar::StatusBar},
};

use super::Workspace;
use crate::config;

impl Workspace {
    /// The activity bar on its side of the panels and chats.
    pub(super) fn render_body(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let right = config::settings(cx).layout.side_panel == vortex_settings::Side::Right;
        let bar = self.render_activity_bar(window, cx);
        let main = gpui_kit::div()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(self.render_main(window, cx));
        let (before, after) = if right { (None, bar) } else { (bar, None) };
        h_flex()
            .flex_1()
            .min_h_0()
            .children(before)
            .child(main)
            .children(after)
    }

    pub(super) fn render_status_bar(
        &self,
        root: &Path,
        active_file: Option<String>,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        StatusBar::new()
            .left(self.process_meter.clone())
            .left(root.display().to_string())
            .right(active_file.unwrap_or_default())
            .when_some(self.shown_cli_version(cx), |bar, version| {
                bar.right(version)
            })
            .when_some(Self::render_update(cx), StatusBar::right)
    }
}
