//! Closing tabs without losing what was typed.

use gpui_kit::prelude::*;
use gpui_kit::{
    Context, Entity, Window,
    component::{
        WindowExt as _,
        button::{Button, ButtonVariants as _},
        dialog::DialogFooter,
    },
};

use super::{EditorPane, document::Document};

impl EditorPane {
    /// Closes the active tab, asking first when it has unsaved changes.
    pub fn close_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(document) = self.active_document().cloned() else {
            return;
        };
        if document.read(cx).is_dirty() {
            ask(&document, window, cx);
        } else {
            self.remove(&document, window, cx);
        }
    }

    pub(super) fn close(
        &mut self,
        document: &Entity<Document>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Shown first, so it is clear which file the question is about.
        if let Some(ix) = self.tabs.iter().position(|tab| tab.document == *document) {
            self.activate(ix, window, cx);
            self.close_active(window, cx);
        }
    }

    pub(super) fn remove(
        &mut self,
        document: &Entity<Document>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ix) = self.tabs.iter().position(|tab| tab.document == *document) else {
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

    fn save_and_close(document: &Entity<Document>, window: &mut Window, cx: &mut Context<Self>) {
        let save = document.update(cx, |document, cx| document.save(window, cx));
        let document = document.clone();
        cx.spawn_in(window, async move |this, cx| {
            // Left open when it could not be written.
            if save.await {
                let _ = this.update_in(cx, |this, window, cx| this.remove(&document, window, cx));
            }
        })
        .detach();
    }
}

fn ask(document: &Entity<Document>, window: &mut Window, cx: &mut Context<EditorPane>) {
    let title = document.read(cx).title().clone();
    let pane = cx.entity().downgrade();
    let document = document.clone();
    window.open_alert_dialog(cx, move |alert, _, _| {
        let saved = (pane.clone(), document.clone());
        let discarded = (pane.clone(), document.clone());
        let footer = DialogFooter::new()
            .child(Button::new("discard-changes").label("Don't Save").on_click(
                move |_, window, cx| {
                    window.close_dialog(cx);
                    let (pane, document) = &discarded;
                    let _ = pane.update(cx, |pane, cx| pane.remove(document, window, cx));
                },
            ))
            .child(
                Button::new("cancel-close")
                    .label("Cancel")
                    .on_click(|_, window, cx| window.close_dialog(cx)),
            )
            .child(
                Button::new("save-changes")
                    .primary()
                    .label("Save")
                    .on_click(move |_, window, cx| {
                        window.close_dialog(cx);
                        let (pane, document) = &saved;
                        let _ = pane.update(cx, |_, cx| {
                            EditorPane::save_and_close(document, window, cx);
                        });
                    }),
            );
        alert
            .title(format!("Save the changes to {title}?"))
            .description("They will be lost if you don't save them.")
            .footer(footer)
    });
}
