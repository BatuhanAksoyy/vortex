//! The activity bar on the window's left edge, which switches what the panel
//! beside the chats shows. It can be hidden and made wider, and its buttons
//! grow with it.

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
use kiln_chat::scale::ScaledButton as _;

use super::Workspace;
use crate::side_panel::{DEFAULT_BAR, Page};

/// The root text size the widths are meant at.
const DEFAULT_REM: f32 = 14.;
/// The button's and icon's shares of the bar's width.
const BUTTON_SHARE: f32 = 0.7;
const ICON_SHARE: f32 = 0.38;

const PAGES: [(Page, &str, Lucide, &str); 3] = [
    (Page::Files, "activity-files", Lucide::FolderTree, "Files"),
    (Page::Search, "activity-search", Lucide::Search, "Search"),
    (Page::Git, "activity-git", Lucide::GitBranch, "Changes"),
];

/// Dragging the bar's edge.
struct ResizeBar;

impl Workspace {
    fn resize_bar(&mut self, width: f32, cx: &mut Context<Self>) {
        self.side_panel.set_bar_width(width);
        self.save_side_panel(cx);
        cx.notify();
    }

    pub(super) fn render_panel_toggle(
        &self,
        cx: &Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        self.project(cx)?;
        let (icon, tooltip) = if self.side_panel.hidden {
            (Lucide::PanelLeftOpen, "Show Side Panel")
        } else {
            (Lucide::PanelLeftClose, "Hide Side Panel")
        };
        let toggle = Button::new("toggle-side-panel")
            .ghost()
            .control_icon(icon)
            .tooltip(tooltip)
            .on_click(cx.listener(|this, _, window, cx| this.toggle_side_panel(window, cx)));
        Some(toggle)
    }

    pub(super) fn render_activity_bar(
        &self,
        window: &Window,
        cx: &Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        if self.side_panel.hidden || self.project(cx).is_none() {
            return None;
        }
        let theme = cx.theme();
        let zoom = window.rem_size() / px(DEFAULT_REM);
        let width = px(self.side_panel.bar_width() * zoom);
        let buttons: Vec<_> = PAGES
            .iter()
            .map(|(page, id, icon, label)| {
                let page = *page;
                let selected = self.side_open() && self.side_panel.page == page;
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
                    this.resize_bar(width / px(1.) / zoom, cx);
                }),
            )
            .children(buttons)
            .child(
                resize_handle("activity-bar-edge", Axis::Horizontal)
                    .inside(HandleEdge::Trailing)
                    .on_double_click(cx.listener(|this, _, _, cx| {
                        this.resize_bar(DEFAULT_BAR, cx);
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
