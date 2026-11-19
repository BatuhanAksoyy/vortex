use std::path::{Path, PathBuf};

use anyhow::Context as _;
use gpui_kit::prelude::*;
use gpui_kit::{
    Context, Entity, FocusHandle, Focusable, SharedString, Subscription, Window,
    component::{
        ActiveTheme, IconName, Sizable, WindowExt,
        button::{Button, ButtonVariants},
        input::{Editor, EditorState, InputEvent, TabSize},
        notification::Notification,
        tab::{Tab, TabBar},
        v_flex,
    },
};
use kiln_settings::EditorSettings;

use crate::config;

const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
const BINARY_SNIFF_BYTES: usize = 8 * 1024;

struct EditorTab {
    id: u64,
    title: SharedString,
    /// `None` for read-only virtual documents such as the default settings.
    path: Option<PathBuf>,
    editor: Entity<EditorState>,
    dirty: bool,
    _subscription: Subscription,
}

struct Document {
    title: SharedString,
    path: Option<PathBuf>,
    text: String,
    language: &'static str,
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

    pub fn active_path(&self) -> Option<&Path> {
        self.active.and_then(|ix| self.tabs[ix].path.as_deref())
    }

    pub fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(ix) = self
            .tabs
            .iter()
            .position(|t| t.path.as_ref() == Some(&path))
        {
            self.activate(ix, window, cx);
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
                    let document = Document {
                        title: file_title(&path),
                        language: kiln_project::language_for_path(&path).unwrap_or("text"),
                        path: Some(path),
                        text,
                    };
                    this.add_tab(document, window, cx);
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
        if let Some(ix) = self
            .tabs
            .iter()
            .position(|t| t.path.is_none() && t.title == title)
        {
            self.activate(ix, window, cx);
            return;
        }
        let document = Document {
            title: title.into(),
            path: None,
            text: text.to_owned(),
            language: "json",
        };
        self.add_tab(document, window, cx);
    }

    fn add_tab(&mut self, document: Document, window: &mut Window, cx: &mut Context<Self>) {
        let settings = config::settings(cx).editor.clone();
        let editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .language(document.language)
                .line_number(settings.line_numbers)
                .indent_guides(settings.indent_guides)
                .soft_wrap(settings.soft_wrap)
                .tab_size(tab_size(&settings))
                .default_value(document.text)
        });
        let id = self.next_id;
        self.next_id += 1;
        let subscription = cx.subscribe(&editor, move |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.set_dirty(id, true, cx);
            }
        });
        self.tabs.push(EditorTab {
            id,
            title: document.title,
            path: document.path,
            editor,
            dirty: false,
            _subscription: subscription,
        });
        self.activate(self.tabs.len() - 1, window, cx);
    }

    fn set_dirty(&mut self, id: u64, dirty: bool, cx: &mut Context<Self>) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == id)
            && tab.dirty != dirty
        {
            tab.dirty = dirty;
            cx.notify();
        }
    }

    fn activate(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.active = Some(ix);
        self.tabs[ix]
            .editor
            .update(cx, |editor, cx| editor.focus(window, cx));
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
        let Some(tab) = self.active.map(|ix| &self.tabs[ix]) else {
            return;
        };
        let Some(path) = tab.path.clone() else {
            window.push_notification(Notification::warning("This document is read-only."), cx);
            return;
        };
        let (id, text) = (tab.id, tab.editor.read(cx).value().to_string());
        let write = cx.background_spawn(async move { std::fs::write(&path, text).map(|()| path) });
        cx.spawn_in(window, async move |this, cx| {
            let result = write.await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(_) => this.set_dirty(id, false, cx),
                Err(error) => {
                    window.push_notification(
                        Notification::error(format!("Save failed: {error}")),
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    /// Closes the active tab, asking first when it has unsaved changes.
    pub fn close_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.active else {
            return;
        };
        if !self.tabs[ix].dirty {
            self.remove_tab(self.tabs[ix].id, window, cx);
            return;
        }
        let (id, title) = (self.tabs[ix].id, self.tabs[ix].title.clone());
        let pane = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let pane = pane.clone();
            alert
                .title(format!("Discard changes to {title}?"))
                .description("Your unsaved changes will be lost.")
                .on_ok(move |_, window, cx| {
                    let _ = pane.update(cx, |pane, cx| pane.remove_tab(id, window, cx));
                    true
                })
        });
    }

    fn close_tab(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(ix) = self.tabs.iter().position(|t| t.id == id) {
            self.active = Some(ix);
            self.close_active(window, cx);
        }
    }

    fn remove_tab(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.tabs.iter().position(|t| t.id == id) else {
            return;
        };
        self.tabs.remove(ix);
        if self.tabs.is_empty() {
            self.active = None;
            cx.notify();
        } else {
            self.activate(ix.min(self.tabs.len() - 1), window, cx);
        }
    }

    pub fn apply_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let settings = config::settings(cx).editor.clone();
        for tab in &self.tabs {
            tab.editor.update(cx, |editor, cx| {
                editor.set_line_number(settings.line_numbers, window, cx);
                editor.set_soft_wrap(settings.soft_wrap, window, cx);
                editor.set_indent_guides(settings.indent_guides, window, cx);
                editor.set_tab_size(tab_size(&settings), cx);
            });
        }
    }

    fn render_tabs(&self, cx: &mut Context<Self>) -> TabBar {
        let tabs = self.tabs.iter().map(|tab| {
            let id = tab.id;
            let label = if tab.dirty {
                format!("● {}", tab.title)
            } else {
                tab.title.to_string()
            };
            Tab::new().label(label).suffix(
                Button::new(("close-tab", id))
                    .icon(IconName::Close)
                    .ghost()
                    .xsmall()
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.close_tab(id, window, cx)),
                    ),
            )
        });
        TabBar::new("editor-tabs")
            .when_some(self.active, TabBar::selected_index)
            .on_click(cx.listener(|this, ix: &usize, window, cx| this.activate(*ix, window, cx)))
            .children(tabs)
    }
}

fn tab_size(settings: &EditorSettings) -> TabSize {
    TabSize {
        tab_size: usize::from(settings.tab_size),
        hard_tabs: false,
    }
}

fn file_title(path: &Path) -> SharedString {
    path.file_name()
        .map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        )
        .into()
}

fn read_text(path: &Path) -> anyhow::Result<String> {
    let size = std::fs::metadata(path).context("cannot read file")?.len();
    if size > MAX_FILE_BYTES {
        anyhow::bail!("file is larger than {} MB", MAX_FILE_BYTES / 1024 / 1024);
    }
    let bytes = std::fs::read(path).context("cannot read file")?;
    if bytes.iter().take(BINARY_SNIFF_BYTES).any(|b| *b == 0) {
        anyhow::bail!("binary file");
    }
    String::from_utf8(bytes).context("file is not UTF-8 text")
}

impl Focusable for EditorPane {
    fn focus_handle(&self, _: &gpui_kit::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for EditorPane {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = match self.active.map(|ix| &self.tabs[ix]) {
            Some(tab) => Editor::new(&tab.editor)
                .readonly(tab.path.is_none())
                .bordered(false)
                .size_full()
                .font_family(cx.theme().mono_font_family.clone())
                .text_size(cx.theme().mono_font_size)
                .into_any_element(),
            None => v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .text_color(cx.theme().muted_foreground)
                .child("Open a file from the tree or press Ctrl+P")
                .into_any_element(),
        };
        v_flex()
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(cx.theme().background)
            .when(!self.tabs.is_empty(), |this| {
                this.child(self.render_tabs(cx))
            })
            .child(gpui_kit::div().flex_1().min_h_0().child(body))
    }
}
