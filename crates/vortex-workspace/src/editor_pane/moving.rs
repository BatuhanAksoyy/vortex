//! Moving an open file from one editor pane to another, unsaved changes
//! and all, so a file is never open twice with two texts.

use std::path::Path;

use gpui_kit::{Context, Entity, Window};

use super::{EditorPane, EditorTab, Saved, document::Document};

impl EditorPane {
    /// Whether the pane has `path` open.
    pub fn shows(&self, path: &Path, cx: &gpui_kit::App) -> bool {
        self.documents()
            .any(|document| document.read(cx).path() == Some(path))
    }

    /// Takes the shown file out of the pane, to move it to another.
    pub fn take_active(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<Document>> {
        let document = self.active_document()?.clone();
        self.remove(&document, window, cx);
        Some(document)
    }

    /// Shows `document`, taken out of another pane.
    pub fn adopt(
        &mut self,
        document: Entity<Document>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
}
