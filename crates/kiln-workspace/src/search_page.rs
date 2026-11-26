//! The Search page, which the side panel shows beside the chats: text found
//! in the project's files beside the editor, which opens a hit at its line.

mod results;

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use gpui_kit::prelude::*;
use gpui_kit::{
    Context, Entity, FocusHandle, Focusable, Subscription, Task, TestSupportExt,
    UniformListScrollHandle, Window,
    component::{
        ActiveTheme,
        input::{InputEvent, InputState},
        resizable::{ResizableState, h_resizable, resizable_panel},
        v_flex,
    },
};
use kiln_project::Found;

use crate::editor_pane::EditorPane;

const LIST_WIDTH_REMS: f32 = 20.0;
/// Typing faster than this searches once, at the end.
const TYPING: Duration = Duration::from_millis(250);

/// A row of the results: a file, then its hits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    File { first: usize, count: usize },
    Hit(usize),
}

pub struct SearchPage {
    root: PathBuf,
    editor: Entity<EditorPane>,
    files: Arc<[PathBuf]>,
    query: Entity<InputState>,
    match_case: bool,
    found: Found,
    rows: Vec<Row>,
    /// The hit last opened.
    picked: Option<usize>,
    /// The search running, and how to stop it.
    running: Option<(Task<()>, Arc<AtomicBool>)>,
    typing: Task<()>,
    scroll: UniformListScrollHandle,
    focus: FocusHandle,
    /// Held here, not by the window, so the split the user sized outlives
    /// the panel being closed.
    split: Entity<ResizableState>,
    _query: Subscription,
}

impl SearchPage {
    pub fn new(
        root: PathBuf,
        editor: Entity<EditorPane>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("Search in files"));
        let typed = cx.subscribe(&query, |this, _, event: &InputEvent, cx| match event {
            InputEvent::PressEnter { .. } => this.search(cx),
            InputEvent::Change => this.search_soon(cx),
            InputEvent::Blur | InputEvent::Focus => {}
        });
        Self {
            root,
            editor,
            files: Arc::from([]),
            query,
            match_case: false,
            found: Found::default(),
            rows: Vec::new(),
            picked: None,
            running: None,
            typing: Task::ready(()),
            scroll: UniformListScrollHandle::new(),
            focus: cx.focus_handle(),
            split: cx.new(|_| ResizableState::default()),
            _query: typed,
        }
    }

    /// The files to look through, as the project lists them.
    pub fn set_files(&mut self, files: Arc<[PathBuf]>) {
        self.files = files;
    }

    /// Puts the keyboard in the search box, with its text selected.
    pub fn focus_query(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.query.update(cx, |query, cx| {
            query.focus(window, cx);
            query.select_all(window, cx);
        });
    }

    fn search_soon(&mut self, cx: &mut Context<Self>) {
        self.typing = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(TYPING).await;
            let _ = this.update(cx, Self::search);
        });
    }

    fn search(&mut self, cx: &mut Context<Self>) {
        if let Some((_, stop)) = self.running.take() {
            stop.store(true, Ordering::Relaxed);
        }
        let query = self.query.read(cx).value().to_string();
        let (root, files, match_case) = (self.root.clone(), self.files.clone(), self.match_case);
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let found = cx.background_spawn(async move {
            kiln_project::search(&root, &files, &query, match_case, &stopped)
        });
        let task = cx.spawn(async move |this, cx| {
            let found = found.await;
            let _ = this.update(cx, |this, cx| this.show(found, cx));
        });
        self.running = Some((task, stop));
        cx.notify();
    }

    fn show(&mut self, found: Found, cx: &mut Context<Self>) {
        self.running = None;
        self.rows = rows(&found);
        self.found = found;
        self.picked = None;
        self.scroll.scroll_to_item(0, gpui_kit::ScrollStrategy::Top);
        cx.notify();
    }

    fn toggle_case(&mut self, cx: &mut Context<Self>) {
        self.match_case = !self.match_case;
        self.search(cx);
    }

    fn open(&mut self, hit: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(found) = self.found.hits.get(hit) else {
            return;
        };
        let (path, line) = (self.root.join(&found.path), found.line);
        self.picked = Some(hit);
        self.editor.update(cx, |editor, cx| {
            editor.open_at(path, Some(line), window, cx);
        });
        cx.notify();
    }
}

/// Each file's row, followed by its hits' rows.
fn rows(found: &Found) -> Vec<Row> {
    let mut rows = Vec::new();
    let mut at = 0;
    while let Some(first) = found.hits.get(at) {
        let count = found.hits[at..]
            .iter()
            .take_while(|hit| hit.path == first.path)
            .count();
        rows.push(Row::File { first: at, count });
        rows.extend((at..at + count).map(Row::Hit));
        at += count;
    }
    rows
}

impl Focusable for SearchPage {
    fn focus_handle(&self, _: &gpui_kit::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for SearchPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let editor = if self.editor.read(cx).is_empty() {
            v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .text_color(cx.theme().muted_foreground)
                .child("Pick a result to open it.")
                .into_any_element()
        } else {
            self.editor.clone().into_any_element()
        };
        h_resizable("search-page")
            .with_state(&self.split)
            .child(
                resizable_panel()
                    .size(window.rem_size() * LIST_WIDTH_REMS)
                    .fixed()
                    .child(self.render_results(cx)),
            )
            .child(
                resizable_panel().child(
                    v_flex()
                        .id("search-editor")
                        .test_support()
                        .size_full()
                        .child(editor),
                ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_project::Hit;

    fn hit(path: &str) -> Hit {
        Hit {
            path: path.into(),
            line: 0,
            text: String::new(),
            start: 0,
            end: 0,
        }
    }

    #[test]
    fn hits_are_grouped_under_their_file() {
        let found = Found {
            hits: vec![hit("a"), hit("a"), hit("b")],
            truncated: false,
        };
        let expected = [
            Row::File { first: 0, count: 2 },
            Row::Hit(0),
            Row::Hit(1),
            Row::File { first: 2, count: 1 },
            Row::Hit(2),
        ];
        assert_eq!(rows(&found), expected);
    }
}
