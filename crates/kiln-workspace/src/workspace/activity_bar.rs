//! The activity bar on the window's left edge, which switches pages. It can
//! be hidden and made wider, and its buttons grow with it.

use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::{
    Axis, Context, DragMoveEvent, Empty, TestSupportExt, Window,
    assets::IconName as Lucide,
    base::{InteractiveElementExt as _, Selectable as _},
    component::{
        ActiveTheme, Icon, Sizable as _, Size,
        button::{Button, ButtonVariants as _},
        resizable::{HandleEdge, resize_handle},
        v_flex,
    },
    div, px,
};

use super::{Workspace, pages::Page};
use crate::left_panel::DEFAULT_WIDTH;

/// The root text size the widths are meant at.
const DEFAULT_REM: f32 = 14.;
/// The button's and icon's shares of the bar's width.
const BUTTON_SHARE: f32 = 0.84;
const ICON_SHARE: f32 = 0.5;
const SAVE_DELAY: Duration = Duration::from_millis(500);

const PAGES: [(Page, &str, Lucide, &str); 3] = [
    (Page::Chat, "activity-chat", Lucide::MessagesSquare, "Chat"),
    (Page::Files, "activity-files", Lucide::FolderTree, "Files"),
    (Page::Git, "activity-git", Lucide::GitBranch, "Git"),
];

/// Dragging the bar's edge.
struct ResizeBar;

impl Workspace {
    pub fn toggle_left_panel(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.left_panel.hidden = !self.left_panel.hidden;
        self.save_left_panel(cx);
        cx.notify();
    }

    /// Saved once it settles: a drag changes it with every move.
    fn save_left_panel(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.left_panel_file.clone() else {
            return;
        };
        let panel = self.left_panel;
        let settled = cx.background_executor().timer(SAVE_DELAY);
        self.left_panel_save = Some(cx.background_spawn(async move {
            settled.await;
            if let Err(error) = panel.save(&path) {
                log::warn!("cannot keep the left panel's size: {error:#}");
            }
        }));
    }

    fn resize_left_panel(&mut self, width: f32, cx: &mut Context<Self>) {
        self.left_panel.set_width(width);
        self.save_left_panel(cx);
        cx.notify();
    }

    /// Switches to `page`. On the page already shown, it folds the list on
    /// its left, as VSCode does.
    fn press_activity(&mut self, page: Page, window: &mut Window, cx: &mut Context<Self>) {
        if self.page != page {
            self.show_page(page, window, cx);
            return;
        }
        match page {
            Page::Chat => {}
            Page::Files => self
                .pages
                .files
                .update(cx, crate::files_page::FilesPage::toggle_tree),
            Page::Git => self
                .pages
                .git
                .update(cx, crate::git_page::GitPage::toggle_list),
        }
    }

    pub(super) fn render_panel_toggle(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let (icon, tooltip) = if self.left_panel.hidden {
            (Lucide::PanelLeftOpen, "Show Left Panel")
        } else {
            (Lucide::PanelLeftClose, "Hide Left Panel")
        };
        Button::new("toggle-left-panel")
            .ghost()
            .small()
            .icon(icon)
            .tooltip(tooltip)
            .on_click(cx.listener(|this, _, window, cx| this.toggle_left_panel(window, cx)))
    }

    pub(super) fn render_activity_bar(
        &self,
        window: &Window,
        cx: &Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        if self.left_panel.hidden {
            return None;
        }
        let theme = cx.theme();
        let zoom = window.rem_size() / px(DEFAULT_REM);
        let width = px(self.left_panel.width() * zoom);
        let buttons: Vec<_> = PAGES
            .iter()
            .map(|(page, id, icon, label)| {
                let page = *page;
                let selected = self.page == page;
                let button = Button::new(*id)
                    .ghost()
                    .size(width * BUTTON_SHARE)
                    .p_0()
                    // As a child: the button sizes its own icon to itself.
                    .child(Icon::new(*icon).with_size(Size::Size(width * ICON_SHARE)))
                    .selected(selected)
                    .tooltip(*label)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.press_activity(page, window, cx);
                    }));
                div()
                    .relative()
                    .flex()
                    .justify_center()
                    .when(selected, |this| this.child(marker(theme.primary)))
                    .child(button)
            })
            .collect();
        let bar = v_flex()
            .id("activity-bar")
            .test_support()
            .relative()
            .flex_none()
            .h_full()
            .w(width)
            .py_1()
            .gap_1()
            .border_r_1()
            .border_color(theme.border)
            .bg(theme.sidebar)
            .on_drag_move(
                cx.listener(move |this, event: &DragMoveEvent<ResizeBar>, _, cx| {
                    let width = event.event.position.x - event.bounds.left();
                    this.resize_left_panel(width / px(1.) / zoom, cx);
                }),
            )
            .children(buttons)
            .child(
                resize_handle("activity-bar-edge", Axis::Horizontal)
                    .inside(HandleEdge::Trailing)
                    .on_double_click(cx.listener(|this, _, _, cx| {
                        this.resize_left_panel(DEFAULT_WIDTH, cx);
                    }))
                    .on_drag(ResizeBar, |_, _, _, cx| cx.new(|_| Empty)),
            );
        Some(bar)
    }
}

/// The line beside the page shown.
fn marker(color: gpui_kit::Hsla) -> impl IntoElement {
    div()
        .absolute()
        .left_0()
        .top_0()
        .h_full()
        .w(px(2.))
        .bg(color)
}
