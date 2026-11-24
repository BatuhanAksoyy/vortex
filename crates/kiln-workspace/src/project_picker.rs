//! The folders to work in: the page Kiln starts with, and over a chat, how
//! it moves to another project.

use std::path::{Path, PathBuf};

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Context, Entity, EventEmitter, FocusHandle, FontWeight, KeyDownEvent,
    SharedString, TestSupportExt, Window,
    assets::IconName as Lucide,
    component::{
        ActiveTheme, Icon, IconName, Sizable as _,
        button::{Button, ButtonVariants as _},
        h_flex, v_flex,
    },
    div, rems,
};

use crate::project::folder_name;

const WIDTH_REMS: f32 = 36.0;

pub enum ProjectPickerEvent {
    Pick(PathBuf),
    /// Ask for a folder that is not listed.
    Browse,
    Close,
}

pub struct ProjectPicker {
    folders: Vec<PathBuf>,
    /// The folder the chat under the page works in; `None` on the start
    /// page, which has nothing to go back to.
    current: Option<PathBuf>,
    /// The row the arrow keys are on.
    marked: usize,
    focus: FocusHandle,
}

impl EventEmitter<ProjectPickerEvent> for ProjectPicker {}

impl ProjectPicker {
    pub fn new(folders: Vec<PathBuf>, current: Option<PathBuf>, cx: &mut Context<Self>) -> Self {
        Self {
            folders,
            current,
            marked: 0,
            focus: cx.focus_handle(),
        }
    }

    pub fn focus(page: &Entity<Self>, window: &mut Window, cx: &mut App) {
        let focus = page.read(cx).focus.clone();
        window.focus(&focus, cx);
    }

    fn on_key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "escape" if self.current.is_some() => cx.emit(ProjectPickerEvent::Close),
            "down" => self.marked = (self.marked + 1).min(self.folders.len().saturating_sub(1)),
            "up" => self.marked = self.marked.saturating_sub(1),
            "enter" => match self.folders.get(self.marked) {
                Some(folder) => cx.emit(ProjectPickerEvent::Pick(folder.clone())),
                None => cx.emit(ProjectPickerEvent::Browse),
            },
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn top_bar(&self, cx: &Context<Self>) -> impl IntoElement {
        let close = self.current.is_some().then(|| {
            Button::new("projects-close")
                .ghost()
                .small()
                .icon(IconName::Close)
                .tooltip("Close")
                .on_click(cx.listener(|_, _, _, cx| cx.emit(ProjectPickerEvent::Close)))
        });
        h_flex()
            .flex_none()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .flex_1()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Choose a project folder"),
            )
            .child(
                Button::new("projects-browse")
                    .primary()
                    .small()
                    .icon(Lucide::FolderOpen)
                    .label("Open Folder…")
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(ProjectPickerEvent::Browse))),
            )
            .children(close)
    }

    fn row(&self, index: usize, folder: &Path, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let name = folder_name(folder);
        let picked = folder.to_owned();
        let hover = theme.list_hover;
        h_flex()
            .id(SharedString::from(format!("project-{}", folder.display())))
            .test_support()
            .aria_label(SharedString::from(format!("Project {name}")))
            .flex_none()
            .gap_3()
            .px_2()
            .py_1p5()
            .rounded(theme.radius)
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .when(index == self.marked, |this| this.bg(theme.list_active))
            .on_click(cx.listener(move |_, _, _, cx| {
                cx.emit(ProjectPickerEvent::Pick(picked.clone()));
            }))
            .child(Icon::new(Lucide::FolderOpen).text_color(theme.muted_foreground))
            .child(
                v_flex().flex_1().min_w_0().child(name).child(
                    div()
                        .truncate()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(folder.display().to_string()),
                ),
            )
            .when(self.current.as_deref() == Some(folder), |this| {
                this.child(
                    div()
                        .flex_none()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("This session"),
                )
            })
            .into_any_element()
    }

    fn rows(&self, cx: &Context<Self>) -> Vec<AnyElement> {
        if self.folders.is_empty() {
            return vec![
                div()
                    .px_2()
                    .py_3()
                    .text_color(cx.theme().muted_foreground)
                    .child("Folders you work in are listed here.")
                    .into_any_element(),
            ];
        }
        let folders = self.folders.iter().enumerate();
        folders
            .map(|(index, folder)| self.row(index, folder, cx))
            .collect()
    }
}

impl Render for ProjectPicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        v_flex()
            .id("projects")
            .test_support()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .w(rems(WIDTH_REMS))
            .max_w_full()
            .max_h_full()
            .min_h_0()
            .rounded(theme.radius_lg)
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .shadow_lg()
            .text_sm()
            .child(self.top_bar(cx))
            .child(
                v_flex()
                    .id("projects-rows")
                    .min_h_0()
                    .p_2()
                    .gap_0p5()
                    .overflow_y_scroll()
                    .children(self.rows(cx)),
            )
    }
}
