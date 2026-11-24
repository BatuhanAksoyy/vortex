//! One open file: its text, whether it has unsaved changes, and how it
//! differs from the last commit.

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::Context as _;
use gpui_kit::prelude::*;
use gpui_kit::{
    Context, Entity, EventEmitter, SharedString, Subscription, Task, TestSupportExt, Window,
    component::{
        ActiveTheme as _, WindowExt as _,
        diff::{Diff, DiffFile, DiffState},
        h_flex,
        input::{Editor, EditorState, InputEvent, Rope, TabSize},
        notification::Notification,
    },
    div,
};
use kiln_git::LineChange;
use kiln_settings::EditorSettings;

use super::{Saved, changes::Marks, minimap::Minimap};
use crate::config;

const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
const BINARY_SNIFF_BYTES: usize = 8 * 1024;
/// Typing is compared with the last commit once it pauses.
const COMPARE_DEBOUNCE: Duration = Duration::from_millis(150);

/// What to open a document with.
pub struct Contents {
    pub title: SharedString,
    /// `None` for read-only virtual documents such as the default settings.
    pub path: Option<PathBuf>,
    pub text: String,
    pub language: &'static str,
}

pub struct Document {
    title: SharedString,
    path: Option<PathBuf>,
    editor: Entity<EditorState>,
    /// The text on disk, which undoing back to makes it unchanged again.
    saved: Rope,
    dirty: bool,
    /// The text in the last commit; `None` when the file is not in it.
    base: Option<SharedString>,
    marks: Marks,
    minimap: Entity<Minimap>,
    changed: bool,
    /// The changes since the last commit, shown in place of the text.
    diff: Option<Entity<DiffState>>,
    base_load: Option<Task<()>>,
    compare: Option<Task<()>>,
    reload: Option<Task<()>>,
    _edited: Subscription,
}

impl EventEmitter<Saved> for Document {}

impl Document {
    pub fn new(contents: Contents, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let settings = config::settings(cx).editor.clone();
        let editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .language(contents.language)
                .line_number(settings.line_numbers)
                .indent_guides(settings.indent_guides)
                .soft_wrap(settings.soft_wrap)
                .tab_size(tab_size(&settings))
                .default_value(contents.text)
        });
        let edited = cx.subscribe(&editor, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.text_changed(cx);
            }
        });
        let mut this = Self {
            title: contents.title,
            path: contents.path,
            saved: editor.read(cx).text().clone(),
            dirty: false,
            base: None,
            marks: Marks::new(&editor, cx),
            minimap: cx.new(|cx| Minimap::new(editor.clone(), contents.language, cx)),
            changed: false,
            diff: None,
            editor,
            base_load: None,
            compare: None,
            reload: None,
            _edited: edited,
        };
        this.load_base(cx);
        this
    }

    pub fn title(&self) -> &SharedString {
        &self.title
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Whether it differs from the last commit.
    pub fn is_changed(&self) -> bool {
        self.changed
    }

    pub fn shows_changes(&self) -> bool {
        self.diff.is_some()
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.editor
            .update(cx, |editor, cx| editor.focus(window, cx));
    }

    /// Follows the file to the path it was renamed to.
    pub fn moved_to(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.title = file_title(&path);
        self.path = Some(path);
        self.load_base(cx);
        cx.notify();
    }

    pub fn apply_settings(&self, window: &mut Window, cx: &mut Context<Self>) {
        let settings = config::settings(cx).editor.clone();
        self.editor.update(cx, |editor, cx| {
            editor.set_line_number(settings.line_numbers, window, cx);
            editor.set_soft_wrap(settings.soft_wrap, window, cx);
            editor.set_indent_guides(settings.indent_guides, window, cx);
            editor.set_tab_size(tab_size(&settings), cx);
        });
    }

    /// Writes the text to disk; whether it was written.
    pub fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Task<bool> {
        let Some(path) = self.path.clone() else {
            window.push_notification(Notification::warning("This document is read-only."), cx);
            return Task::ready(false);
        };
        let text = self.editor.read(cx).text().clone();
        let written = text.clone();
        let write = cx.background_spawn(async move { std::fs::write(&path, text.to_string()) });
        cx.spawn_in(window, async move |this, cx| {
            let result = write.await;
            this.update_in(cx, |this, window, cx| match result {
                Ok(()) => {
                    this.saved = written;
                    this.check_dirty(cx);
                    cx.emit(Saved);
                    true
                }
                Err(error) => {
                    let message = format!("Save failed: {error}");
                    window.push_notification(Notification::error(message), cx);
                    false
                }
            })
            .unwrap_or(false)
        })
    }

    /// Reads the file and the last commit again: agents and other programs
    /// change both. Unsaved changes are kept.
    pub fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.load_base(cx);
        let Some(path) = self.path.clone().filter(|_| !self.dirty) else {
            return;
        };
        let read = cx.background_spawn(async move { read_text(&path) });
        self.reload = Some(cx.spawn_in(window, async move |this, cx| {
            if let Ok(text) = read.await {
                let _ = this.update_in(cx, |this, window, cx| this.show_disk(&text, window, cx));
            }
        }));
    }

    fn show_disk(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let disk = Rope::from(text);
        if self.dirty || same(&disk, &self.saved) {
            return;
        }
        self.editor.update(cx, |editor, cx| {
            let scroll = editor.scroll_offset();
            editor.set_value(text.to_owned(), window, cx);
            editor.set_scroll_offset(scroll, cx);
        });
        self.saved = self.editor.read(cx).text().clone();
        self.text_changed(cx);
    }

    fn text_changed(&mut self, cx: &mut Context<Self>) {
        self.check_dirty(cx);
        self.minimap.update(cx, Minimap::text_changed);
        self.compare(cx);
    }

    fn check_dirty(&mut self, cx: &mut Context<Self>) {
        let dirty = !same(self.editor.read(cx).text(), &self.saved);
        if self.dirty != dirty {
            self.dirty = dirty;
            cx.notify();
        }
    }

    fn load_base(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.path.clone() else {
            return;
        };
        let read = cx.background_spawn(async move { kiln_git::base_text(&path) });
        self.base_load = Some(cx.spawn(async move |this, cx| {
            let base = read.await;
            let _ = this.update(cx, |this, cx| {
                this.base = base.map(SharedString::from);
                this.compare(cx);
            });
        }));
    }

    /// Finds what changed since the last commit.
    fn compare(&mut self, cx: &mut Context<Self>) {
        let text = self.editor.read(cx).text().clone();
        let base = self.base.clone();
        self.compare = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(COMPARE_DEBOUNCE).await;
            let find = cx.background_spawn(async move {
                let text = text.to_string();
                let changes = base.map(|base| kiln_git::line_changes(&base, &text));
                (changes.unwrap_or_default(), text)
            });
            let (changes, text) = find.await;
            let _ = this.update(cx, |this, cx| this.show_changes(changes, &text, cx));
        }));
    }

    fn show_changes(&mut self, changes: Vec<LineChange>, text: &str, cx: &mut Context<Self>) {
        self.marks.set(&changes, text, cx);
        self.changed = !changes.is_empty();
        self.minimap
            .update(cx, |minimap, cx| minimap.set_changes(changes, cx));
        if !self.changed {
            self.diff = None;
        }
        if let Some(diff) = &self.diff {
            let files = self.diff_files(text);
            diff.update(cx, |diff, cx| diff.set_files(files, cx));
        }
        cx.notify();
    }

    /// Switches between the text and its changes since the last commit.
    pub fn toggle_changes(&mut self, cx: &mut Context<Self>) {
        self.diff = if self.diff.is_none() && self.changed {
            let files = self.diff_files(&self.editor.read(cx).text().to_string());
            Some(cx.new(|cx| DiffState::new(files, cx)))
        } else {
            None
        };
        cx.notify();
    }

    fn diff_files(&self, text: &str) -> Vec<DiffFile> {
        let base = self.base.as_deref().unwrap_or_default();
        let patch = kiln_git::patch(&self.title, base, text);
        DiffFile::parse(&patch).unwrap_or_default()
    }
}

impl Render for Document {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        if let Some(diff) = &self.diff {
            let diff = Diff::new(diff).header_visible(false).size_full();
            return div()
                .id("editor-changes")
                .test_support()
                .size_full()
                .child(diff)
                .into_any_element();
        }
        let editor = Editor::new(&self.editor)
            .readonly(self.path.is_none())
            .bordered(false)
            .size_full()
            .aria_label(format!("Editor {}", self.title))
            .font_family(theme.mono_font_family.clone())
            .text_size(theme.mono_font_size);
        h_flex()
            .size_full()
            .child(div().flex_1().min_w_0().h_full().child(editor))
            .child(self.minimap.clone())
            .into_any_element()
    }
}

fn same(one: &Rope, other: &Rope) -> bool {
    one.len() == other.len() && one == other
}

fn tab_size(settings: &EditorSettings) -> TabSize {
    TabSize {
        tab_size: usize::from(settings.tab_size),
        hard_tabs: false,
    }
}

pub fn file_title(path: &Path) -> SharedString {
    path.file_name()
        .map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        )
        .into()
}

pub fn read_text(path: &Path) -> anyhow::Result<String> {
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
