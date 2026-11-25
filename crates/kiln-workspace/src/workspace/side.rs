//! The panel on the chats' left: the active session's files or changes.

use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Context, Entity, FontWeight, Pixels, TestSupportExt, Window,
    assets::IconName as Lucide,
    component::{
        ActiveTheme, IconName,
        button::{Button, ButtonVariants as _},
        h_flex,
        resizable::{ResizableState, h_resizable, resizable_panel},
        v_flex,
    },
    div, px,
};
use kiln_chat::scale::{BAR_HEIGHT, ScaledButton as _};

use super::Workspace;
use crate::{
    files_page::FilesPage,
    git_page::GitPage,
    project::Project,
    side_panel::{MAX_BAR, Page},
};

const SAVE_DELAY: Duration = Duration::from_millis(500);
/// A panel's share of the window until the user sizes it.
const DEFAULT_SHARE: f32 = 0.35;
/// Narrower, neither the chat nor a panel can be used.
pub(super) const MIN_WIDTH: f32 = 280.;

impl Workspace {
    pub(super) fn show_page(&mut self, page: Page, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.project(cx).cloned() else {
            Self::needs_project(window, cx);
            return;
        };
        self.side_panel.page = page;
        self.side_panel.open = true;
        self.save_side_panel(cx);
        self.refresh_project(window, cx);
        match page {
            Page::Git => {
                let git = project.read(cx).git_page().clone();
                git.update(cx, |git, cx| git.focus(window, cx));
            }
            Page::Search => {
                let search = project.read(cx).search_page().clone();
                search.update(cx, |search, cx| search.focus_query(window, cx));
            }
            Page::Files => {}
        }
        cx.notify();
    }

    /// Shows `page`, or closes the panel when it shows it already.
    pub(super) fn press_activity(
        &mut self,
        page: Page,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.side_panel.open && self.side_panel.page == page {
            self.close_side(window, cx);
        } else {
            self.show_page(page, window, cx);
        }
    }

    fn close_side(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.side_panel.open = false;
        self.save_side_panel(cx);
        self.focus_active_chat(window, cx);
        cx.notify();
    }

    /// Focuses the chat, making room for it when the panel covers it.
    pub fn show_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.uncover_chats();
        self.focus_active_chat(window, cx);
        cx.notify();
    }

    /// Makes room beside the panel if it fills the window. A closed one
    /// covers nothing and reopens as it was.
    pub(super) fn uncover_chats(&mut self) {
        if self.side_open() {
            self.side_maximized = false;
        }
    }

    pub fn show_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_page(Page::Files, window, cx);
    }

    pub fn show_git(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_page(Page::Git, window, cx);
    }

    pub fn show_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_page(Page::Search, window, cx);
    }

    /// Hides the panel and the bar that opens it, or shows them again.
    pub fn toggle_side_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.side_panel.hidden = !self.side_panel.hidden;
        self.save_side_panel(cx);
        self.refresh_project(window, cx);
        cx.notify();
    }

    fn toggle_maximized(&mut self, cx: &mut Context<Self>) {
        self.side_maximized = !self.side_maximized;
        cx.notify();
    }

    pub(super) fn side_open(&self) -> bool {
        self.side_panel.open && !self.side_panel.hidden
    }

    /// Reads the active project again while the panel shows it.
    pub(super) fn refresh_project(&self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.side_open() {
            return;
        }
        if let Some(project) = self.project(cx).cloned() {
            project.update(cx, |project, cx| project.refresh(window, cx));
        }
    }

    /// Saved once it settles: a drag changes it with every move.
    pub(super) fn save_side_panel(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.side_panel_file.clone() else {
            return;
        };
        let panel = self.side_panel;
        let settled = cx.background_executor().timer(SAVE_DELAY);
        self.side_panel_save = Some(cx.background_spawn(async move {
            settled.await;
            if let Err(error) = panel.save(&path) {
                log::warn!("cannot keep the side panel: {error:#}");
            }
        }));
    }

    fn toggle_list(&self, cx: &mut Context<Self>) {
        let Some(project) = self.project(cx).cloned() else {
            return;
        };
        let project = project.read(cx);
        let (files, git) = (project.files_page().clone(), project.git_page().clone());
        match self.side_panel.page {
            Page::Files => files.update(cx, FilesPage::toggle_tree),
            Page::Git => git.update(cx, GitPage::toggle_list),
            Page::Search => {}
        }
    }

    /// The side panel while it is open, the chats and the browser.
    pub(super) fn render_main(&self, window: &Window, cx: &Context<Self>) -> AnyElement {
        if let Some(start) = self.render_start_page() {
            return start;
        }
        let Some(project) = self.project(cx).filter(|_| self.side_open()) else {
            return self.render_chats(window, cx);
        };
        let side = self.render_side(project, cx);
        if self.side_maximized {
            return side.into_any_element();
        }
        let chats = self.render_chats(window, cx);
        let this = cx.entity().downgrade();
        let resized = move |state: &Entity<ResizableState>, _: &mut Window, cx: &mut App| {
            let width = state.read(cx).sizes().first().copied().map(f32::from);
            let _ = this.update(cx, |this, cx| {
                this.side_panel.page_width = width;
                this.save_side_panel(cx);
            });
        };
        // The browser keeps its own least width beside the chat's.
        let panes = 1. + f32::from(u8::from(self.side_panel.browser_open));
        h_resizable("side-and-chats")
            .child(
                resizable_panel()
                    .size(self.side_width(window))
                    .fixed()
                    .size_range(px(MIN_WIDTH)..Pixels::MAX)
                    .child(side),
            )
            .child(
                resizable_panel()
                    .size_range(px(MIN_WIDTH * panes)..Pixels::MAX)
                    .child(chats),
            )
            .on_resize(resized)
            .into_any_element()
    }

    /// Kept from a wider window, a panel may leave the chat no room, so both
    /// are held to what this one has.
    pub(super) fn side_width(&self, window: &Window) -> Pixels {
        let panes = 1. + f32::from(u8::from(self.side_panel.browser_open));
        let width = self.side_panel.page_width;
        share(width, px(MIN_WIDTH * panes + MAX_BAR), window)
    }

    fn render_side(
        &self,
        project: &Entity<Project>,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let shown = project.read(cx);
        let page = match self.side_panel.page {
            Page::Files => shown.files_page().clone().into_any_element(),
            Page::Git => shown.git_page().clone().into_any_element(),
            Page::Search => shown.search_page().clone().into_any_element(),
        };
        v_flex()
            .id("side-panel")
            .test_support()
            .size_full()
            .border_r_1()
            .border_color(cx.theme().border)
            .child(self.render_side_header(shown, cx))
            .child(div().flex_1().min_h_0().child(page))
    }

    fn render_side_header(
        &self,
        project: &Project,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let (title, list) = match self.side_panel.page {
            Page::Files => ("Files", "Show or Hide the Tree"),
            Page::Git => ("Changes", "Show or Hide the List"),
            Page::Search => ("Search", "Show or Hide the Results"),
        };
        let (icon, resize) = if self.side_maximized {
            (Lucide::Minimize, "Show the Chat")
        } else {
            (Lucide::Maximize, "Fill the Window")
        };
        let button = |id, tooltip| Button::new(id).ghost().tooltip(tooltip);
        h_flex()
            .flex_none()
            .gap_1()
            .px_2()
            .h(BAR_HEIGHT)
            .border_b_1()
            .border_color(theme.border)
            .child(
                button("side-toggle-list", list)
                    .control_icon(Lucide::PanelLeft)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_list(cx))),
            )
            .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
            .child(
                div()
                    .id("side-project")
                    .test_support()
                    .aria_label(gpui_kit::SharedString::from(format!(
                        "Showing {}",
                        project.name()
                    )))
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .pl_1()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(project.name()),
            )
            .child(
                button("side-maximize", resize)
                    .control_icon(icon)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_maximized(cx))),
            )
            .child(
                button("side-close", "Close Panel")
                    .control_icon(IconName::Close)
                    .on_click(cx.listener(|this, _, window, cx| this.close_side(window, cx))),
            )
    }
}

/// A panel's width: `kept` or its default share, leaving `rest` of the window
/// to the others.
pub(super) fn share(kept: Option<f32>, rest: Pixels, window: &Window) -> Pixels {
    let window = window.viewport_size().width;
    let width = kept.map_or(window * DEFAULT_SHARE, px);
    width.min(window - rest).max(px(MIN_WIDTH))
}
