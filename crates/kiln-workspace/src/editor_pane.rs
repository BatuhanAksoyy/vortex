//! The editor's tabs, each an open file.

mod changes;
mod closing;
mod document;
mod minimap;

use std::path::{Path, PathBuf};

use gpui_kit::prelude::*;
use gpui_kit::{
    Context, Entity, EventEmitter, FocusHandle, Focusable, SharedString, Subscription,
    TestSupportExt, Window,
    assets::IconName as Lucide,
    base::Selectable as _,
    component::{
        ActiveTheme, Disableable as _, IconName, Sizable, WindowExt,
        button::{Button, ButtonVariants},
        h_flex,
        notification::Notification,
        tab::{Tab, TabBar},
        v_flex,
    },
    div,
};

use crate::file_icon;
use document::{Contents, Document, file_title, read_text};

/// A file was written.
pub struct Saved;

impl EventEmitter<Saved> for EditorPane {}

struct EditorTab {
    id: u64,
    document: Entity<Document>,
    _subscriptions: [Subscription; 2],
}

pub struct EditorPane {
    tabs: Vec<EditorTab>,
    active: Option<usize>,
    next_id: u64,
    focus_handle: FocusHandle,
}

impl EditorPane {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            tabs: Vec::new(),
            active: None,
            next_id: 0,
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn active_path<'a>(&self, cx: &'a gpui_kit::App) -> Option<&'a Path> {
        self.active_document()?.read(cx).path()
    }

    fn active_document(&self) -> Option<&Entity<Document>> {
        Some(&self.tabs.get(self.active?)?.document)
    }

    fn documents(&self) -> impl Iterator<Item = &Entity<Document>> {
        self.tabs.iter().map(|tab| &tab.document)
    }

    pub fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.open_at(path, None, window, cx);
    }

    /// Opens `path` with the cursor at the start of `line`, from 0.
    pub fn open_at(
        &mut self,
        path: PathBuf,
        line: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let open = self
            .documents()
            .position(|document| document.read(cx).path() == Some(&path));
        if let Some(ix) = open {
            self.activate(ix, window, cx);
            self.go_to_line(line, window, cx);
            return;
        }
        let read = cx.background_spawn({
            let path = path.clone();
            async move { read_text(&path) }
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = read.await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(text) => {
                    let contents = Contents {
                        title: file_title(&path),
                        language: kiln_project::language_for_path(&path).unwrap_or("text"),
                        path: Some(path),
                        text,
                    };
                    this.add_tab(contents, window, cx);
                    this.go_to_line(line, window, cx);
                }
                Err(error) => {
                    let message = format!("Cannot open {}: {error:#}", path.display());
                    window.push_notification(Notification::error(message), cx);
                }
            });
        })
        .detach();
    }

    /// Opens a read-only JSON document that has no file on disk.
    pub fn open_virtual_json(
        &mut self,
        title: &'static str,
        text: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let open = self.documents().position(|document| {
            let document = document.read(cx);
            document.path().is_none() && document.title() == title
        });
        if let Some(ix) = open {
            self.activate(ix, window, cx);
            return;
        }
        let contents = Contents {
            title: title.into(),
            path: None,
            text: text.to_owned(),
            language: "json",
        };
        self.add_tab(contents, window, cx);
    }

    fn add_tab(&mut self, contents: Contents, window: &mut Window, cx: &mut Context<Self>) {
        // Opened twice in a row, the second read ends after the first.
        let open = contents.path.as_deref().and_then(|path| {
            self.documents()
                .position(|document| document.read(cx).path() == Some(path))
        });
        if let Some(ix) = open {
            self.activate(ix, window, cx);
            return;
        }
        let document = cx.new(|cx| Document::new(contents, window, cx));
        let subscriptions = [
            cx.observe(&document, |_, _, cx| cx.notify()),
            cx.subscribe(&document, |_, _, _: &Saved, cx| cx.emit(Saved)),
        ];
        self.tabs.push(EditorTab {
            id: self.next_id,
            document,
            _subscriptions: subscriptions,
        });
        self.next_id += 1;
        self.activate(self.tabs.len() - 1, window, cx);
    }

    /// Ignores a stale index: a tab's close button also clicks the tab bar,
    /// which then reports the tab it just removed.
    fn go_to_line(&self, line: Option<u32>, window: &mut Window, cx: &mut Context<Self>) {
        if let (Some(line), Some(document)) = (line, self.active_document()) {
            document.update(cx, |document, cx| document.go_to_line(line, window, cx));
        }
    }

    fn activate(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get(ix) else {
            return;
        };
        tab.document
            .update(cx, |document, cx| document.focus(window, cx));
        self.active = Some(ix);
        cx.notify();
    }

    pub fn activate_relative(&mut self, step: isize, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(active), Ok(len)) = (self.active, isize::try_from(self.tabs.len())) else {
            return;
        };
        let Ok(active) = isize::try_from(active) else {
            return;
        };
        if let Ok(next) = usize::try_from((active + step).rem_euclid(len)) {
            self.activate(next, window, cx);
        }
    }

    pub fn save_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(document) = self.active_document() {
            let save = document.update(cx, |document, cx| document.save(window, cx));
            save.detach();
        }
    }

    pub fn toggle_changes(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(document) = self.active_document() {
            document.update(cx, Document::toggle_changes);
        }
    }

    /// Reads the open files again, as they may have changed on disk.
    pub fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for document in self.documents() {
            document.update(cx, |document, cx| document.refresh(window, cx));
        }
    }

    /// Follows the files in `from` to `to`, which it was renamed to.
    pub fn renamed(&mut self, from: &Path, to: &Path, cx: &mut Context<Self>) {
        for document in self.documents() {
            let path = document.read(cx).path();
            if let Some(path) = path.and_then(|path| renamed_path(path, from, to)) {
                document.update(cx, |document, cx| document.moved_to(path, cx));
            }
        }
    }

    /// Closes the files in `path`, which was deleted, but those with unsaved
    /// changes.
    pub fn deleted(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        let gone: Vec<Entity<Document>> = self
            .documents()
            .filter(|document| {
                let document = document.read(cx);
                let inside = document.path().is_some_and(|open| open.starts_with(path));
                inside && !document.is_dirty()
            })
            .cloned()
            .collect();
        for document in gone {
            self.remove(&document, window, cx);
        }
    }

    pub fn apply_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for document in self.documents() {
            document.update(cx, |document, cx| document.apply_settings(window, cx));
        }
    }

    fn render_tabs(&self, cx: &mut Context<Self>) -> TabBar {
        let tabs: Vec<Tab> = self.tabs.iter().map(|tab| render_tab(tab, cx)).collect();
        TabBar::new("editor-tabs")
            .when_some(self.active, TabBar::selected_index)
            .on_click(cx.listener(|this, ix: &usize, window, cx| this.activate(*ix, window, cx)))
            .children(tabs)
            .suffix(self.changes_button(cx))
    }

    fn changes_button(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let document = self.active_document().map(|document| document.read(cx));
        let changed = document.is_some_and(Document::is_changed);
        let shown = document.is_some_and(Document::shows_changes);
        let tooltip = match (changed, shown) {
            (false, _) => "No changes since the last commit",
            (true, false) => "Show changes since the last commit",
            (true, true) => "Show the file",
        };
        h_flex().px_2().child(
            Button::new("editor-toggle-changes")
                .ghost()
                .small()
                .icon(Lucide::FileDiff)
                .selected(shown)
                .disabled(!changed)
                .tooltip(tooltip)
                .on_click(cx.listener(|this, _, window, cx| this.toggle_changes(window, cx))),
        )
    }
}

/// Where `path` is after `from`, it or a folder above it, became `to`.
fn renamed_path(path: &Path, from: &Path, to: &Path) -> Option<PathBuf> {
    let inside = path.strip_prefix(from).ok()?;
    // Joining nothing would end the path with a slash.
    if inside.as_os_str().is_empty() {
        return Some(to.to_path_buf());
    }
    Some(to.join(inside))
}

fn render_tab(tab: &EditorTab, cx: &Context<EditorPane>) -> Tab {
    let read = tab.document.read(cx);
    let title: SharedString = if read.is_dirty() {
        format!("● {}", read.title()).into()
    } else {
        read.title().clone()
    };
    let icon = file_icon::file(read.title(), cx);
    let closed = tab.document.clone();
    Tab::new()
        .label(title)
        .prefix(div().pl_2().child(icon))
        .suffix(
            Button::new(("close-editor-tab", tab.id))
                .icon(IconName::Close)
                .ghost()
                .xsmall()
                .on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.close(&closed, window, cx);
                })),
        )
}

impl Focusable for EditorPane {
    fn focus_handle(&self, _: &gpui_kit::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for EditorPane {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = match self.active_document() {
            Some(document) => document.clone().into_any_element(),
            None => v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .text_color(cx.theme().muted_foreground)
                .child("Open a file from the tree or press Ctrl+P")
                .into_any_element(),
        };
        v_flex()
            .id("editor-pane")
            .test_support()
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(cx.theme().background)
            .when(!self.tabs.is_empty(), |this| {
                this.child(self.render_tabs(cx))
            })
            .child(div().flex_1().min_h_0().child(body))
    }
}
