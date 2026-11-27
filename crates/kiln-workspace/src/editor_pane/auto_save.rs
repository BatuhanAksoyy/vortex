//! Saving an edited file once typing stops, when `files.auto_save` says so.

use std::time::Duration;

use gpui_kit::{AppContext as _, Context, component::input::Rope};
use kiln_settings::AutoSave;

use super::{Saved, document::Document};
use crate::config;

impl Document {
    /// Saves the text once it has not changed for the settings' delay; a
    /// further change starts the wait again.
    pub(super) fn save_after_typing(&mut self, cx: &mut Context<Self>) {
        let files = &config::settings(cx).files;
        let (Some(path), true) = (self.text_path(), files.auto_save == AutoSave::AfterDelay) else {
            self.auto_save = None;
            return;
        };
        if !self.is_dirty() {
            self.auto_save = None;
            return;
        }
        let delay = Duration::from_millis(files.auto_save_delay);
        self.auto_save = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let Ok(text) = this.update(cx, |this, cx| this.text(cx)) else {
                return;
            };
            let written = text.clone();
            let file = path.clone();
            let write = cx.background_spawn(async move { std::fs::write(&file, text.to_string()) });
            match write.await {
                Ok(()) => drop(this.update(cx, |this, cx| this.auto_saved(written, cx))),
                Err(error) => log::warn!("cannot save {}: {error}", path.display()),
            }
        }));
    }

    fn auto_saved(&mut self, text: Rope, cx: &mut Context<Self>) {
        self.saved_as(text, cx);
        cx.emit(Saved);
    }
}
