//! A second editor beside the first. Split Editor moves the shown file
//! across, and the pane puts itself away when its last file closes. A file
//! is open in one pane only, so its text is never kept twice.

use std::path::{Path, PathBuf};

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Context, Entity, Subscription, TestSupportExt, Window,
    component::resizable::{ResizableState, h_resizable, resizable_panel},
    div,
};

use super::FilesPage;
use crate::editor_pane::EditorPane;

pub(super) struct Split {
    aside: Entity<EditorPane>,
    /// The pane last clicked in, which opened files and the editor's
    /// commands go to.
    aside_active: bool,
    sizes: Entity<ResizableState>,
    _observing: Subscription,
}

impl Split {
    pub(super) fn new(aside: Entity<EditorPane>, cx: &mut Context<FilesPage>) -> Self {
        Self {
            _observing: cx.observe(&aside, |this, _, cx| this.follow_editor(cx)),
            aside,
            aside_active: false,
            sizes: cx.new(|_| ResizableState::default()),
        }
    }
}

impl FilesPage {
    /// The pane the editor's commands go to.
    pub fn active_editor(&self, cx: &App) -> &Entity<EditorPane> {
        let split = &self.split;
        if split.aside_active && !split.aside.read(cx).is_empty() {
            &split.aside
        } else {
            &self.editor
        }
    }

    /// Both panes, the first one first.
    pub fn editors(&self) -> [Entity<EditorPane>; 2] {
        [self.editor.clone(), self.split.aside.clone()]
    }

    pub(super) fn renamed_file(&self, from: &Path, to: &Path, cx: &mut Context<Self>) {
        for editor in self.editors() {
            editor.update(cx, |editor, cx| editor.renamed(from, to, cx));
        }
    }

    pub(super) fn deleted_file(&self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        for editor in self.editors() {
            editor.update(cx, |editor, cx| editor.deleted(path, window, cx));
        }
    }

    fn other_editor(&self, cx: &App) -> &Entity<EditorPane> {
        if self.active_editor(cx) == &self.editor {
            &self.split.aside
        } else {
            &self.editor
        }
    }

    /// Moves the shown file to the other pane.
    pub fn split_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (from, to) = (
            self.active_editor(cx).clone(),
            self.other_editor(cx).clone(),
        );
        let Some(document) = from.update(cx, |pane, cx| pane.take_active(window, cx)) else {
            return;
        };
        to.update(cx, |pane, cx| pane.adopt(document, window, cx));
        self.split.aside_active = to == self.split.aside;
        cx.notify();
    }

    /// Opens `path` in the active pane at `line`, or shows it in the pane
    /// it is open in.
    pub fn open_in_editor(
        &mut self,
        path: PathBuf,
        line: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let other = self.other_editor(cx).clone();
        let pane = if other.read(cx).shows(&path, cx) {
            self.split.aside_active = other == self.split.aside;
            other
        } else {
            self.active_editor(cx).clone()
        };
        pane.update(cx, |pane, cx| pane.open_at(path, line, window, cx));
        cx.notify();
    }

    /// The editor, and the one beside it while it holds a file.
    pub(super) fn render_editors(&self, main: AnyElement, cx: &Context<Self>) -> AnyElement {
        if self.split.aside.read(cx).is_empty() {
            return main;
        }
        let pane = |element: AnyElement, aside: bool| {
            div()
                .id(if aside { "files-aside" } else { "files-main" })
                .test_support()
                .size_full()
                .capture_any_mouse_down(cx.listener(move |this, _, _, _| {
                    this.split.aside_active = aside;
                }))
                .child(element)
        };
        h_resizable("files-editors")
            .with_state(&self.split.sizes)
            .child(resizable_panel().child(pane(main, false)))
            .child(resizable_panel().child(pane(self.split.aside.clone().into_any_element(), true)))
            .into_any_element()
    }
}
