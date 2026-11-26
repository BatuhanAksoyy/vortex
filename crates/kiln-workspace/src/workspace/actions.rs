//! Command handlers. Each public method backs one command in
//! `crate::commands::COMMANDS`.

use std::path::{Path, PathBuf};

use gpui_kit::prelude::*;
use gpui_kit::{
    Context, Window,
    component::{WindowExt, notification::Notification},
};

use super::Workspace;
use crate::{
    config,
    editor_pane::EditorPane,
    pickers::{self, CommandPalette, QuickOpen, QuickOpenEvent},
    side_panel::Page,
};

impl Workspace {
    pub fn quick_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.project(cx).cloned() else {
            Self::needs_project(window, cx);
            return;
        };
        let files = project.read(cx).files();
        let view = cx.new(|cx| QuickOpen::new(files, window, cx));
        cx.subscribe_in(
            &view,
            window,
            move |this, _, event, window, cx| match event {
                QuickOpenEvent::Open(relative) => {
                    window.close_dialog(cx);
                    let root = project.read(cx).root().to_owned();
                    this.open_absolute(root.join(relative), window, cx);
                }
            },
        )
        .detach();
        pickers::open_in_dialog(view.clone(), window, cx);
        view.update(cx, |view, cx| view.focus(window, cx));
    }

    pub fn show_commands(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.new(|cx| CommandPalette::new(window, cx));
        pickers::open_in_dialog(view.clone(), window, cx);
        view.update(cx, |view, cx| view.focus(window, cx));
    }

    /// The commit settings of the active chat, on `scope`'s tab.
    pub(super) fn commit_settings(
        &mut self,
        scope: kiln_settings::Scope,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let opened = match &self.active_chat {
            Some(panel) => panel.read(cx).commit_settings(scope, panel, cx),
            None => crate::commit_settings::Opened {
                scope: kiln_settings::Scope::Global,
                project: None,
                chat: None,
                own: serde_json::Value::Null,
                agents: vec![self.backend.name().to_owned()],
                models: Vec::new(),
            },
        };
        crate::commit_settings::open(opened, window, cx);
    }

    pub fn open_commit_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_settings(kiln_settings::Scope::Chat, window, cx);
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
        self.with_editor(window, cx, |pane, window, cx| {
            pane.open_virtual_json(title, text, window, cx);
        });
        self.show_page(Page::Files, window, cx);
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

    /// Runs `command` on the active session's editor.
    fn with_editor(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
        command: impl FnOnce(&mut EditorPane, &mut Window, &mut Context<EditorPane>),
    ) {
        if let Some(project) = self.project(cx).cloned() {
            let editor = project.read(cx).editor().clone();
            editor.update(cx, |pane, cx| command(pane, window, cx));
        }
    }

    pub fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.with_editor(window, cx, EditorPane::save_active);
    }

    pub fn toggle_changes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.with_editor(window, cx, EditorPane::toggle_changes);
    }

    pub fn close_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.with_editor(window, cx, EditorPane::close_active);
    }

    pub fn next_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.with_editor(window, cx, |pane, window, cx| {
            pane.activate_relative(1, window, cx);
        });
    }

    pub fn previous_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.with_editor(window, cx, |pane, window, cx| {
            pane.activate_relative(-1, window, cx);
        });
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
