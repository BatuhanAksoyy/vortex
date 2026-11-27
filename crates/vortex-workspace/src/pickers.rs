//! Quick open (fuzzy file search) and the command palette, both shown in a
//! dialog built on gpui-kit's `Command`.

use std::{path::PathBuf, sync::Arc};

use gpui_kit::{
    App, Context, Entity, EventEmitter, SharedString, TestSupportExt, Window,
    component::{
        WindowExt,
        command::{Command, CommandItem, CommandState},
    },
    div,
    prelude::*,
    rems,
};

use crate::commands::COMMANDS;

const MAX_RESULTS: usize = 200;
const DIALOG_WIDTH_REMS: f32 = 40.0;

pub enum QuickOpenEvent {
    Open(PathBuf),
}

pub struct QuickOpen {
    state: Entity<CommandState>,
    files: Arc<[PathBuf]>,
    matches: Vec<usize>,
}

impl EventEmitter<QuickOpenEvent> for QuickOpen {}

impl QuickOpen {
    pub fn new(files: Arc<[PathBuf]>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            state: cx.new(|cx| CommandState::new(window, cx)),
            files,
            matches: Vec::new(),
        };
        this.search("");
        this
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.state.update(cx, |state, cx| state.focus(window, cx));
    }

    fn search(&mut self, query: &str) {
        let candidates: Vec<&std::path::Path> = self.files.iter().map(PathBuf::as_path).collect();
        self.matches = vortex_project::fuzzy_match(query, &candidates, MAX_RESULTS)
            .into_iter()
            .map(|m| m.index)
            .collect();
    }
}

impl QuickOpen {
    fn confirm(&self, row: usize, cx: &mut Context<Self>) {
        if let Some(&ix) = self.matches.get(row) {
            cx.emit(QuickOpenEvent::Open(self.files[ix].clone()));
        }
    }
}

impl Render for QuickOpen {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let items = self.matches.iter().map(|&ix| {
            CommandItem::new().label(SharedString::from(
                self.files[ix].to_string_lossy().into_owned(),
            ))
        });
        let on_query = cx.entity().downgrade();
        let on_confirm = on_query.clone();
        let command = Command::new(&self.state)
            .filterable(false)
            .bordered(false)
            .placeholder("Search files by name")
            .items(items)
            .on_query(move |query, _, cx| {
                let _ = on_query.update(cx, |this, cx| {
                    this.search(query);
                    cx.notify();
                });
            })
            .on_confirm(move |index, _, cx| {
                let _ = on_confirm.update(cx, |this, cx| this.confirm(index.row, cx));
            })
            .max_h(rems(28.));
        div().id("quick-open").test_support().child(command)
    }
}

pub struct CommandPalette {
    state: Entity<CommandState>,
}

impl CommandPalette {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            state: cx.new(|cx| CommandState::new(window, cx)),
        }
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.state.update(cx, |state, cx| state.focus(window, cx));
    }
}

impl Render for CommandPalette {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let items = COMMANDS.iter().map(|spec| {
            CommandItem::new()
                .label(spec.label)
                .keywords([spec.id])
                .action(spec.action())
        });
        Command::new(&self.state)
            .bordered(false)
            .highlight_matches(true)
            .placeholder("Type a command")
            .items(items)
            .on_confirm(|_, window, cx| window.close_dialog(cx))
            .max_h(rems(28.))
    }
}

/// Shows `view` in a borderless dialog sized for a picker.
pub fn open_in_dialog<V: Render>(view: Entity<V>, window: &mut Window, cx: &mut App) {
    window.open_dialog(cx, move |dialog, window, _| {
        let view = view.clone();
        dialog
            .close_button(false)
            .p_0()
            .w(window.rem_size() * DIALOG_WIDTH_REMS)
            .content(move |content, _, _| content.child(view.clone()))
    });
}
