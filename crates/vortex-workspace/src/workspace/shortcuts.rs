//! The keyboard shortcuts page beside the work, in the settings page's place.

use gpui_kit::prelude::*;
use gpui_kit::{
    Context, Focusable as _, Window, component::WindowExt as _,
    component::notification::Notification,
};

use super::Workspace;
use crate::shortcuts_page::{ShortcutsPage, ShortcutsPageEvent};

impl Workspace {
    /// Shows the shortcuts page, or closes it when it is shown.
    pub fn toggle_shortcuts_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.shortcuts_page.is_some() {
            self.close_shortcuts_page(window, cx);
        } else {
            self.open_shortcuts_page(window, cx);
        }
    }

    pub fn open_shortcuts_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((page, _)) = &self.shortcuts_page {
            page.focus_handle(cx).focus(window, cx);
            return;
        }
        let Some(file) = vortex_settings::user_keybindings_path() else {
            let note = Notification::error("There is no folder for your keyboard shortcuts.");
            window.push_notification(note, cx);
            return;
        };
        self.settings_page = None;
        let page = cx.new(|cx| ShortcutsPage::new(file, window, cx));
        let events = cx.subscribe_in(&page, window, |this, _, event, window, cx| match event {
            ShortcutsPageEvent::Changed => this.reload_config(window, cx),
            ShortcutsPageEvent::Close => this.close_shortcuts_page(window, cx),
            ShortcutsPageEvent::OpenFile => {
                this.close_shortcuts_page(window, cx);
                this.open_user_keybindings(window, cx);
            }
        });
        page.focus_handle(cx).focus(window, cx);
        self.shortcuts_page = Some((page, events));
        cx.notify();
    }

    pub(super) fn close_shortcuts_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.shortcuts_page.take().is_some() {
            self.focus_handle.focus(window, cx);
            cx.notify();
        }
    }

    pub(super) fn reread_shortcuts_page(&self, cx: &mut Context<Self>) {
        if let Some((page, _)) = &self.shortcuts_page {
            page.update(cx, ShortcutsPage::reread);
        }
    }
}
