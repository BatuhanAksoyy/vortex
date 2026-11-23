//! Command handlers. Each public method backs one command in
//! `crate::commands::COMMANDS`.

use std::path::{Path, PathBuf};

use gpui_kit::prelude::*;
use gpui_kit::{
    Context, PathPromptOptions, Window,
    component::{WindowExt, notification::Notification},
};

use super::Workspace;
use crate::{
    config,
    editor_pane::EditorPane,
    pickers::{self, CommandPalette, QuickOpen, QuickOpenEvent},
};

impl Workspace {
    pub fn quick_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.new(|cx| QuickOpen::new(self.files.clone(), window, cx));
        cx.subscribe_in(&view, window, |this, _, event, window, cx| match event {
            QuickOpenEvent::Open(relative) => {
                window.close_dialog(cx);
                this.open_relative(relative, window, cx);
            }
        })
        .detach();
        pickers::open_in_dialog(view.clone(), window, cx);
        view.update(cx, |view, cx| view.focus(window, cx));
    }

    pub fn show_commands(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.new(|cx| CommandPalette::new(window, cx));
        pickers::open_in_dialog(view.clone(), window, cx);
        view.update(cx, |view, cx| view.focus(window, cx));
    }

    pub fn toggle_sidebar(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.sidebar_visible = !self.sidebar_visible;
        cx.notify();
    }

    pub fn toggle_editor_pane(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.editor_visible = !self.editor_visible;
        cx.notify();
    }

    pub fn open_user_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_user_file(kiln_settings::user_settings_path(), "{\n}\n", window, cx);
    }

    pub fn open_user_keybindings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_user_file(kiln_settings::user_keybindings_path(), "[\n]\n", window, cx);
    }

    pub fn open_default_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = kiln_settings::DEFAULT_SETTINGS;
        self.open_virtual("Default Settings", text, window, cx);
    }

    pub fn open_default_keybindings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = kiln_keymap::DEFAULT_KEYBINDINGS;
        self.open_virtual("Default Keybindings", text, window, cx);
    }

    fn open_virtual(
        &mut self,
        title: &'static str,
        text: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editor_visible = true;
        self.editor_pane.update(cx, |pane, cx| {
            pane.open_virtual_json(title, text, window, cx);
        });
        cx.notify();
    }

    /// Opens a user config file, creating it with `template` when missing.
    fn open_user_file(
        &mut self,
        path: Option<PathBuf>,
        template: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = path else {
            window.push_notification(Notification::error("No user config directory."), cx);
            return;
        };
        if let Err(error) = ensure_file(&path, template) {
            let message = format!("Cannot create {}: {error}", path.display());
            window.push_notification(Notification::error(message), cx);
            return;
        }
        self.open_absolute(path, window, cx);
    }

    pub fn prompt_open_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: None,
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            if let Some(root) = paths.into_iter().next() {
                let _ = this.update_in(cx, |this, window, cx| this.open_folder(root, window, cx));
            }
        })
        .detach();
    }

    fn open_folder(&mut self, root: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.chat = super::new_chat(
            root.clone(),
            self.backend.clone(),
            self.accounts.clone(),
            window,
            cx,
        );
        self.root = root;
        self.editor_pane = cx.new(EditorPane::new);
        self.load_project(window, cx);
        cx.notify();
    }

    pub fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editor_pane
            .update(cx, |pane, cx| pane.save_active(window, cx));
    }

    pub fn close_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editor_pane
            .update(cx, |pane, cx| pane.close_active(window, cx));
    }

    pub fn next_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editor_pane
            .update(cx, |pane, cx| pane.activate_relative(1, window, cx));
    }

    pub fn previous_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editor_pane
            .update(cx, |pane, cx| pane.activate_relative(-1, window, cx));
    }

    pub fn zoom_in(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        config::adjust_zoom(Some(1.0), cx);
    }

    pub fn zoom_out(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        config::adjust_zoom(Some(-1.0), cx);
    }

    pub fn zoom_reset(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        config::adjust_zoom(None, cx);
    }
}

fn ensure_file(path: &Path, template: &str) -> std::io::Result<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, template)
}
