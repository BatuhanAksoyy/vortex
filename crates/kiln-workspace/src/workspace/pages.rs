//! The page the window shows: the chats, the files or git, each over the
//! whole window.

use std::path::{Path, PathBuf};

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, Context, Entity, Subscription, Window,
    component::{WindowExt, notification::Notification},
};
use kiln_project::ListOptions;

use super::{MAX_PROJECT_ENTRIES, Scan, Workspace};
use crate::{
    config,
    editor_pane::{EditorPane, Saved},
    files_page::{FilesChanged, FilesPage},
    git_page::{GitPage, GitPageEvent},
    repo_status::{RepoStatus, StatusRead},
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    Chat,
    Files,
    Git,
}

/// The Files and Git pages of one project, and what they show.
pub struct ProjectPages {
    pub repo: Entity<RepoStatus>,
    pub editor: Entity<EditorPane>,
    pub files: Entity<FilesPage>,
    pub git: Entity<GitPage>,
    _subscriptions: [Subscription; 4],
}

impl ProjectPages {
    pub fn new(root: &Path, window: &Window, cx: &mut Context<Workspace>) -> Self {
        let repo = cx.new(|_| RepoStatus::new(root.to_owned()));
        let editor = cx.new(EditorPane::new);
        let files = cx.new(|cx| FilesPage::new(root.to_owned(), editor.clone(), repo.clone(), cx));
        let git = cx.new(|cx| GitPage::new(repo.clone(), cx));
        let saved = repo.clone();
        let subscriptions = [
            cx.subscribe(&editor, move |_, _, _: &Saved, cx| {
                saved.update(cx, RepoStatus::refresh);
            }),
            // Read together: what a file is compared with may have changed.
            cx.subscribe_in(&repo, window, |this, _, _: &StatusRead, window, cx| {
                this.pages
                    .editor
                    .update(cx, |editor, cx| editor.refresh(window, cx));
            }),
            cx.subscribe_in(&files, window, |this, _, _: &FilesChanged, window, cx| {
                this.refresh_project(window, cx);
            }),
            cx.subscribe_in(&git, window, |this, _, event, window, cx| match event {
                GitPageEvent::OpenFile(path) => this.open_absolute(path.clone(), window, cx),
            }),
        ];
        Self {
            repo,
            editor,
            files,
            git,
            _subscriptions: subscriptions,
        }
    }
}

impl Workspace {
    pub(super) fn show_page(&mut self, page: Page, window: &mut Window, cx: &mut Context<Self>) {
        self.page = page;
        self.refresh_project(window, cx);
        if page == Page::Git {
            self.pages.git.update(cx, |git, cx| git.focus(window, cx));
        }
        cx.notify();
    }

    /// Reads the project's files and git status again while a page shows
    /// them: agents and other programs change both behind Kiln's back.
    pub(super) fn refresh_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.page == Page::Chat {
            return;
        }
        self.pages.repo.update(cx, RepoStatus::refresh);
        if self.page == Page::Files {
            self.scan_files(window, cx);
        }
    }

    pub fn show_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_page(Page::Chat, window, cx);
    }

    pub fn show_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_page(Page::Files, window, cx);
    }

    pub fn show_git(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_page(Page::Git, window, cx);
    }

    pub(super) fn scan_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let root = self.root.clone();
        let options = ListOptions {
            exclude: config::settings(cx).files.exclude.clone(),
            limit: MAX_PROJECT_ENTRIES,
        };
        let scan = cx.background_spawn(async move {
            kiln_project::list_files(&root, &options).map(|list| {
                let files = list.files().map(Path::to_path_buf).collect();
                (files, kiln_project::build_tree(&list), list.truncated)
            })
        });
        // Replaced by the next scan, so a slow one cannot land after it.
        self.scan = Some(cx.spawn_in(window, async move |this, cx| {
            let result = scan.await;
            let _ = this.update_in(cx, |this, window, cx| this.apply_scan(result, window, cx));
        }));
    }

    fn apply_scan(
        &mut self,
        result: anyhow::Result<Scan>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok((files, tree, truncated)) => {
                let first = self.files.is_empty();
                self.files = files;
                self.pages
                    .files
                    .update(cx, |page, cx| page.set_files(tree, cx));
                if truncated && first {
                    let message = format!("Showing the first {MAX_PROJECT_ENTRIES} entries.");
                    window.push_notification(Notification::info(message), cx);
                }
            }
            Err(error) => {
                let message = format!("Cannot list project files: {error:#}");
                window.push_notification(Notification::error(message), cx);
            }
        }
    }

    pub(super) fn open_relative(
        &mut self,
        relative: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_absolute(self.root.join(relative), window, cx);
    }

    /// Opens `path` in the editor and shows it on the Files page.
    pub(super) fn open_absolute(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Git names files by the repository's real path.
        let path = match self.root.canonicalize() {
            Ok(real) => path
                .strip_prefix(real)
                .map_or_else(|_| path.clone(), |relative| self.root.join(relative)),
            Err(_) => path,
        };
        if let Ok(relative) = path.strip_prefix(&self.root) {
            self.pages
                .files
                .update(cx, |files, cx| files.reveal(relative, cx));
        }
        self.pages
            .editor
            .update(cx, |pane, cx| pane.open_path(path, window, cx));
        self.show_page(Page::Files, window, cx);
    }

    pub(super) fn render_page(&self) -> AnyElement {
        match self.page {
            Page::Chat => self.dock.clone().into_any_element(),
            Page::Files => self.pages.files.clone().into_any_element(),
            Page::Git => self.pages.git.clone().into_any_element(),
        }
    }
}
