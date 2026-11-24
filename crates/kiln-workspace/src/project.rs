//! A folder sessions work in: its stored conversations, files and git
//! status. Sessions in one folder share it, so a file is open, and edited,
//! once.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use gpui_kit::prelude::*;
use gpui_kit::{
    Context, Entity, EventEmitter, Subscription, Task, Window,
    component::{WindowExt, notification::Notification},
};
use kiln_agent::AgentBackend;
use kiln_project::ListOptions;

use crate::{
    config,
    editor_pane::{EditorPane, Saved},
    files_page::{FilesChanged, FilesPage},
    git_page::{GitPage, GitPageEvent},
    repo_status::{RepoStatus, StatusRead},
    session_list::{SessionList, SessionListEvent},
};

const MAX_ENTRIES: usize = 100_000;

/// The files, as a list and as a tree, and whether the listing hit the
/// entry limit.
type Scan = (Arc<[PathBuf]>, Vec<kiln_project::TreeNode>, bool);

pub enum ProjectEvent {
    /// Its conversations were listed, or could not be.
    SessionsListed,
    /// A file picked on the Git page, to be shown in the editor.
    OpenFile(PathBuf),
}

pub struct Project {
    root: PathBuf,
    sessions: Entity<SessionList>,
    repo: Entity<RepoStatus>,
    editor: Entity<EditorPane>,
    files_page: Entity<FilesPage>,
    git_page: Entity<GitPage>,
    files: Arc<[PathBuf]>,
    /// Lists the files; replaced by the next scan, so a slow one cannot
    /// land after it.
    scan: Option<Task<()>>,
    _subscriptions: [Subscription; 5],
}

impl EventEmitter<ProjectEvent> for Project {}

impl Project {
    pub fn new(root: PathBuf, window: &Window, cx: &mut Context<Self>) -> Self {
        let sessions = cx.new(|_| SessionList::default());
        let repo = cx.new(|_| RepoStatus::new(root.clone()));
        let editor = cx.new(EditorPane::new);
        let files_page =
            cx.new(|cx| FilesPage::new(root.clone(), editor.clone(), repo.clone(), cx));
        let git_page = cx.new(|cx| GitPage::new(repo.clone(), cx));
        let subscriptions = [
            cx.subscribe(&sessions, |_, _, event, cx| match event {
                SessionListEvent::Loaded => cx.emit(ProjectEvent::SessionsListed),
            }),
            cx.subscribe(&editor, |this, _, _: &Saved, cx| {
                this.repo.update(cx, RepoStatus::refresh);
            }),
            // Read together: what a file is compared with may have changed.
            cx.subscribe_in(&repo, window, |this, _, _: &StatusRead, window, cx| {
                this.editor
                    .update(cx, |editor, cx| editor.refresh(window, cx));
            }),
            cx.subscribe_in(
                &files_page,
                window,
                |this, _, _: &FilesChanged, window, cx| {
                    this.refresh(window, cx);
                },
            ),
            cx.subscribe(&git_page, |_, _, event, cx| match event {
                GitPageEvent::OpenFile(path) => cx.emit(ProjectEvent::OpenFile(path.clone())),
            }),
        ];
        Self {
            root,
            sessions,
            repo,
            editor,
            files_page,
            git_page,
            files: Arc::from([]),
            scan: None,
            _subscriptions: subscriptions,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn name(&self) -> String {
        folder_name(&self.root)
    }

    pub fn sessions(&self) -> &Entity<SessionList> {
        &self.sessions
    }

    pub fn editor(&self) -> &Entity<EditorPane> {
        &self.editor
    }

    pub fn files_page(&self) -> &Entity<FilesPage> {
        &self.files_page
    }

    pub fn git_page(&self) -> &Entity<GitPage> {
        &self.git_page
    }

    pub fn files(&self) -> Arc<[PathBuf]> {
        self.files.clone()
    }

    pub fn reload_sessions(&self, backend: &dyn AgentBackend, cx: &mut Context<Self>) {
        let listing = backend.past_sessions(&self.root);
        self.sessions
            .update(cx, |sessions, cx| sessions.reload(listing, cx));
    }

    /// Reads the files and git status again: agents and other programs
    /// change both behind Kiln's back.
    pub fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.repo.update(cx, RepoStatus::refresh);
        self.scan_files(window, cx);
    }

    pub fn scan_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let root = self.root.clone();
        let options = ListOptions {
            exclude: config::settings(cx).files.exclude.clone(),
            limit: MAX_ENTRIES,
        };
        let scan = cx.background_spawn(async move {
            kiln_project::list_files(&root, &options).map(|list| {
                let files = list.files().map(Path::to_path_buf).collect();
                (files, kiln_project::build_tree(&list), list.truncated)
            })
        });
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
                self.files_page
                    .update(cx, |page, cx| page.set_files(tree, cx));
                if truncated && first {
                    let message = format!("Showing the first {MAX_ENTRIES} entries.");
                    window.push_notification(Notification::info(message), cx);
                }
            }
            Err(error) => {
                let message = format!("Cannot list project files: {error:#}");
                window.push_notification(Notification::error(message), cx);
            }
        }
    }

    /// Opens `path` in the editor and selects it in the tree.
    pub fn open(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        // Git names files by the repository's real path.
        let path = match self.root.canonicalize() {
            Ok(real) => path
                .strip_prefix(real)
                .map_or_else(|_| path.clone(), |relative| self.root.join(relative)),
            Err(_) => path,
        };
        if let Ok(relative) = path.strip_prefix(&self.root) {
            self.files_page
                .update(cx, |files, cx| files.reveal(relative, cx));
        }
        self.editor
            .update(cx, |pane, cx| pane.open_path(path, window, cx));
    }
}

pub fn folder_name(root: &Path) -> String {
    let name = root.file_name().unwrap_or(root.as_os_str());
    name.to_string_lossy().into_owned()
}
