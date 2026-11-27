//! The Git page, which the side panel shows beside the chats: the
//! branch, what changed, the diff of the change picked, and staging and
//! committing by hand.

mod branching;
mod changes;
mod committing;
mod diff_pane;
mod history;
mod pushing;

use std::{collections::HashSet, path::PathBuf};

use gpui_kit::prelude::*;
use gpui_kit::{
    Context, Entity, EventEmitter, FocusHandle, KeyDownEvent, ScrollHandle, SharedString,
    Subscription, Task, TestSupportExt, Window,
    component::{
        diff::{DiffFile, DiffState},
        input::TextareaState,
        resizable::{ResizableState, h_resizable, resizable_panel, v_resizable},
    },
    div,
};
use vortex_git::{Area, Branch, Change, Commit};

use crate::repo_status::{RepoStatus, StatusRead};

const LIST_WIDTH_REMS: f32 = 17.0;

pub enum GitPageEvent {
    /// Show the file at this path in the editor.
    OpenFile(PathBuf),
    /// The commit settings, on the project's tab.
    CommitSettings,
}

pub struct GitPage {
    repo: Entity<RepoStatus>,
    selected: Option<Change>,
    /// The sections folded away.
    collapsed: HashSet<Area>,
    list_hidden: bool,
    list_focus: FocusHandle,
    list_scroll: ScrollHandle,
    diff: Entity<DiffState>,
    /// The patch `diff` shows, so reading it again unchanged keeps the
    /// scroll position; `None` while it loads.
    patch: Option<Result<String, String>>,
    diff_load: Option<Task<()>>,
    commits: Vec<Commit>,
    /// The commit whose changes the diff shows, instead of a change.
    picked_commit: Option<String>,
    log_load: Option<Task<()>>,
    message: Entity<TextareaState>,
    /// The staging or commit running; the others wait for it.
    write: Option<Task<()>>,
    write_error: Option<SharedString>,
    branches: Vec<Branch>,
    branch_load: Option<Task<()>>,
    new_branch: Option<branching::NewBranch>,
    /// The list beside the diff, and the changes above the history; held
    /// here, not by the window, so they outlive the panel being closed.
    splits: [Entity<ResizableState>; 2],
    _subscriptions: [Subscription; 2],
}

impl EventEmitter<GitPageEvent> for GitPage {}

impl GitPage {
    pub fn new(repo: Entity<RepoStatus>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let status_read = cx.subscribe(&repo, |this, _, _: &StatusRead, cx| this.follow(cx));
        let message = committing::new_message(window, cx);
        let typed = committing::follow_message(&message, window, cx);
        Self {
            repo,
            selected: None,
            collapsed: HashSet::new(),
            list_hidden: false,
            list_focus: cx.focus_handle(),
            list_scroll: ScrollHandle::new(),
            diff: cx.new(|cx| DiffState::new([], cx)),
            patch: None,
            diff_load: None,
            commits: Vec::new(),
            picked_commit: None,
            log_load: None,
            message,
            write: None,
            write_error: None,
            branches: Vec::new(),
            branch_load: None,
            new_branch: None,
            splits: [(); 2].map(|()| cx.new(|_| ResizableState::default())),
            _subscriptions: [status_read, typed],
        }
    }

    /// The settings of the project the page shows.
    fn settings(&self, cx: &gpui_kit::App) -> vortex_settings::Settings {
        crate::config::settings_at(self.repo.read(cx).project(), cx)
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.list_focus.focus(window, cx);
    }

    /// Keeps the change picked after a new reading: the same one, or the
    /// same file in another area, as after staging it.
    fn follow(&mut self, cx: &mut Context<Self>) {
        self.read_log(cx);
        self.read_branches(cx);
        if self.picked_commit.is_some() {
            return;
        }
        let changes = self.ordered(cx);
        let kept = self.selected.as_ref().and_then(|selected| {
            let same_file = |change: &&Change| change.path == selected.path;
            let mut same = changes.iter().filter(same_file);
            let same_area = same.clone().find(|change| change.area == selected.area);
            same_area.or_else(|| same.next())
        });
        let next = kept.or_else(|| changes.first()).cloned();
        self.select(next, cx);
    }

    /// Every change, section by section as listed.
    fn ordered(&self, cx: &Context<Self>) -> Vec<Change> {
        let status = self.repo.read(cx).status();
        let mut changes = status.map_or_else(Vec::new, |status| status.changes.clone());
        changes.sort_by_key(|change| change.area);
        changes
    }

    /// The changes in sections that are open, which the arrows go through.
    fn reachable(&self, cx: &Context<Self>) -> Vec<Change> {
        let mut changes = self.ordered(cx);
        changes.retain(|change| !self.collapsed.contains(&change.area));
        changes
    }

    fn select(&mut self, change: Option<Change>, cx: &mut Context<Self>) {
        let same = match (&self.selected, &change) {
            (Some(old), Some(new)) => old.area == new.area && old.path == new.path,
            (old, new) => old.is_none() && new.is_none(),
        };
        self.picked_commit = None;
        self.selected.clone_from(&change);
        if !same {
            self.patch = None;
        }
        self.diff_load = None;
        let root = self
            .repo
            .read(cx)
            .status()
            .map(|status| status.root.clone());
        if let (Some(change), Some(root)) = (change, root) {
            let read = cx.background_spawn(async move { vortex_git::diff(&root, &change) });
            self.diff_load = Some(cx.spawn(async move |this, cx| {
                let patch = read.await;
                let _ = this.update(cx, |this, cx| this.show_diff(patch, cx));
            }));
        }
        cx.notify();
    }

    fn show_diff(&mut self, patch: anyhow::Result<String>, cx: &mut Context<Self>) {
        let patch = patch.map_err(|error| format!("Cannot read the diff: {error:#}"));
        if self.patch.as_ref() == Some(&patch) {
            return;
        }
        let parsed = patch.and_then(|patch| match DiffFile::parse(&patch) {
            Ok(files) => Ok((patch, files)),
            Err(error) => Err(format!("Cannot show the diff: {error}")),
        });
        self.patch = Some(parsed.map(|(patch, files)| {
            self.diff.update(cx, |diff, cx| diff.set_files(files, cx));
            patch
        }));
        cx.notify();
    }

    fn step(&mut self, forward: bool, cx: &mut Context<Self>) {
        let changes = self.reachable(cx);
        let at = self.position(&changes);
        let next = match (at, forward) {
            (Some(at), true) => at + 1,
            (Some(at), false) => at.wrapping_sub(1),
            (None, _) => 0,
        };
        if let Some(change) = changes.get(next) {
            self.scroll_to(change, cx);
            self.select(Some(change.clone()), cx);
        }
    }

    fn position(&self, changes: &[Change]) -> Option<usize> {
        let selected = self.selected.as_ref()?;
        changes.iter().position(|change| change == selected)
    }

    /// Hides the list of changes, leaving the diff, or shows it again.
    pub fn toggle_list(&mut self, cx: &mut Context<Self>) {
        self.list_hidden = !self.list_hidden;
        cx.notify();
    }

    fn toggle_section(&mut self, area: Area, cx: &mut Context<Self>) {
        if !self.collapsed.remove(&area) {
            self.collapsed.insert(area);
        }
        cx.notify();
    }

    fn open_selected(&self, cx: &mut Context<Self>) {
        let path = self.openable(cx);
        if let Some(path) = path {
            cx.emit(GitPageEvent::OpenFile(path));
        }
    }

    /// The picked change's file, unless it is gone.
    fn openable(&self, cx: &Context<Self>) -> Option<PathBuf> {
        let change = self.selected.as_ref()?;
        let root = &self.repo.read(cx).status()?.root;
        (change.kind != vortex_git::Kind::Deleted).then(|| root.join(&change.path))
    }

    fn on_list_key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "down" => self.step(true, cx),
            "up" => self.step(false, cx),
            "enter" => self.open_selected(cx),
            _ => return,
        }
        cx.stop_propagation();
    }
}

impl Render for GitPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let list = (!self.list_hidden).then(|| self.render_changes(cx));
        div().id("git-page").test_support().size_full().child(
            h_resizable("git-panels")
                .with_state(&self.splits[0])
                .child(
                    resizable_panel()
                        .size(window.rem_size() * LIST_WIDTH_REMS)
                        .fixed()
                        .visible(!self.list_hidden)
                        .children(list.map(|list| {
                            v_resizable("git-sides")
                                .with_state(&self.splits[1])
                                .child(resizable_panel().child(list))
                                .child(resizable_panel().child(self.render_history(cx)))
                        })),
                )
                .child(resizable_panel().child(self.render_diff(cx))),
        )
    }
}
