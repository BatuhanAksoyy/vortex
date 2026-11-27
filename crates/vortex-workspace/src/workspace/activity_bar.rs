//! The activity bar on the window's edge, which switches what the panel
//! beside the chats shows and holds the gear at its foot. It can be hidden,
//! moved to the right and made wider, and its buttons grow with it; its
//! pages are dragged to reorder them.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, Axis, Context, DragMoveEvent, Empty, Pixels, TestSupportExt, Window,
    assets::IconName as Lucide,
    base::{InteractiveElementExt as _, Selectable as _},
    component::{
        ActiveTheme, Icon, Sizable as _, Size, WindowExt as _,
        button::{Button, ButtonVariants as _},
        notification::Notification,
        resizable::{HandleEdge, resize_handle},
        v_flex,
    },
    div, px,
};
use serde_json::Value;
use vortex_chat::scale::ScaledButton as _;
use vortex_settings::{ActivityPage, Scope, Side};

use super::Workspace;
use crate::{
    config,
    side_panel::{DEFAULT_BAR, Page},
};

/// The root text size the widths are meant at.
const DEFAULT_REM: f32 = 14.;
/// The button's and icon's shares of the bar's width.
const BUTTON_SHARE: f32 = 0.7;
const ICON_SHARE: f32 = 0.38;

/// What the bar shows for each page: its page, element id, icon and name.
fn shown(page: ActivityPage) -> (Page, &'static str, Lucide, &'static str) {
    match page {
        ActivityPage::Files => (Page::Files, "activity-files", Lucide::FolderTree, "Files"),
        ActivityPage::Search => (Page::Search, "activity-search", Lucide::Search, "Search"),
        ActivityPage::Changes => (Page::Git, "activity-git", Lucide::GitBranch, "Changes"),
    }
}

/// Dragging the bar's edge.
struct ResizeBar;

/// A page being dragged to another place on the bar.
#[derive(Clone, Copy)]
struct DraggedPage(ActivityPage);

impl Render for DraggedPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (_, _, icon, _) = shown(self.0);
        div()
            .p_1()
            .rounded_md()
            .bg(cx.theme().secondary)
            .child(Icon::new(icon))
    }
}

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
        let layout = &config::settings(cx).layout;
        if !layout.title_bar_buttons {
            return None;
        }
        let right = layout.side_panel == Side::Right;
        let icon = match (self.side_panel.hidden, right) {
            (true, false) => Lucide::PanelLeftOpen,
            (false, false) => Lucide::PanelLeftClose,
            (true, true) => Lucide::PanelRightOpen,
            (false, true) => Lucide::PanelRightClose,
        };
        let tooltip = if self.side_panel.hidden {
            "Show Side Panel"
        } else {
            "Hide Side Panel"
        };
        let toggle = Button::new("toggle-side-panel")
            .ghost()
            .control_icon(icon)
            .tooltip(tooltip)
            .on_click(cx.listener(|this, _, window, cx| this.toggle_side_panel(window, cx)));
        Some(toggle)
    }

    /// Puts `moved` where `onto` is, and keeps the order in the user's
    /// settings.
    fn move_page(
        &mut self,
        moved: ActivityPage,
        onto: ActivityPage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut pages = config::settings(cx).layout.activity_pages();
        let (Some(from), Some(to)) = (
            pages.iter().position(|page| *page == moved),
            pages.iter().position(|page| *page == onto),
        ) else {
            return;
        };
        let page = pages.remove(from);
        pages.insert(to, page);
        let order: Vec<Value> = pages.into_iter().map(|page| name(page).into()).collect();
        let key = ["layout", "activity_order"];
        let written = vortex_settings::Layers::read(None, Value::Null)
            .and_then(|mut layers| layers.set(Scope::Global, &key, Some(Value::Array(order))));
        match written {
            Ok(()) => self.reload_config(window, cx),
            Err(error) => {
                let error = format!("Cannot keep the order: {error:#}");
                window.push_notification(Notification::error(error), cx);
            }
        }
    }

    pub(super) fn render_activity_bar(
        &self,
        window: &Window,
        cx: &Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let layout = &config::settings(cx).layout;
        // Without a project the pages have nothing to show; the start page
        // offers the gear's settings and updates itself.
        if self.side_panel.hidden || !layout.activity_bar || self.project(cx).is_none() {
            return None;
        }
        let right = layout.side_panel == Side::Right;
        let theme = cx.theme();
        let zoom = window.rem_size() / px(DEFAULT_REM);
        let width = px(self.side_panel.bar_width() * zoom);
        let buttons: Vec<AnyElement> = layout
            .activity_pages()
            .into_iter()
            .map(|page| self.render_page_button(page, width, right, cx))
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
            .map(|bar| {
                if right {
                    bar.border_l_1()
                } else {
                    bar.border_r_1()
                }
            })
            .border_color(theme.border)
            .bg(theme.sidebar)
            .on_drag_move(
                cx.listener(move |this, event: &DragMoveEvent<ResizeBar>, _, cx| {
                    let width = if right {
                        event.bounds.right() - event.event.position.x
                    } else {
                        event.event.position.x - event.bounds.left()
                    };
                    this.resize_bar(width / px(1.) / zoom, cx);
                }),
            )
            .children(buttons)
            .child(div().flex_1())
            .child(div().flex().justify_center().child(Self::render_gear(
                width * BUTTON_SHARE,
                width * ICON_SHARE,
                right,
                cx,
            )))
            .child(Self::render_bar_edge(right, cx));
        Some(bar)
    }

    /// The edge toward the panels, dragged to size the bar.
    fn render_bar_edge(right: bool, cx: &Context<Self>) -> impl IntoElement + use<> {
        let edge = if right {
            HandleEdge::Leading
        } else {
            HandleEdge::Trailing
        };
        resize_handle("activity-bar-edge", Axis::Horizontal)
            .inside(edge)
            .on_double_click(cx.listener(|this, _, _, cx| {
                this.resize_bar(DEFAULT_BAR, cx);
            }))
            .on_drag(ResizeBar, |_, _, _, cx| cx.new(|_| Empty))
    }

    fn render_page_button(
        &self,
        shown_page: ActivityPage,
        width: Pixels,
        right: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let (page, id, icon, label) = shown(shown_page);
        let selected = self.side_open() && self.side_panel.page == page;
        let button = Button::new(id)
            .ghost()
            .size(width * BUTTON_SHARE)
            .p_0()
            // As a child: the button sizes its own icon to itself.
            .child(Icon::new(icon).with_size(Size::Size(width * ICON_SHARE)))
            .selected(selected)
            .tooltip(label)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.press_activity(page, window, cx);
            }));
        div()
            .id(format!("{id}-slot"))
            .relative()
            .flex()
            .justify_center()
            .when(selected, |this| this.child(marker(theme.primary, right)))
            .on_drag(DraggedPage(shown_page), |dragged, _, _, cx| {
                cx.new(|_| *dragged)
            })
            .drag_over::<DraggedPage>(|this, _, _, cx| this.bg(cx.theme().drop_target))
            .on_drop(cx.listener(move |this, dragged: &DraggedPage, window, cx| {
                this.move_page(dragged.0, shown_page, window, cx);
            }))
            .child(button)
            .into_any_element()
    }
}

/// The page's name in `layout.activity_order`.
const fn name(page: ActivityPage) -> &'static str {
    match page {
        ActivityPage::Files => "files",
        ActivityPage::Search => "search",
        ActivityPage::Changes => "changes",
    }
}

/// The line beside the page shown, on the bar's outer edge.
fn marker(color: gpui_kit::Hsla, right: bool) -> impl IntoElement {
    div()
        .absolute()
        .map(|it| if right { it.right_0() } else { it.left_0() })
        .top_0()
        .h_full()
        .w(px(2.))
        .bg(color)
}
