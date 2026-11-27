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
use vortex_project::ListOptions;

use crate::{
    config,
    editor_pane::{EditorPane, Saved},
    files_page::{FilesChanged, FilesPage},
    git_page::{GitPage, GitPageEvent},
    repo_status::{RepoStatus, StatusRead},
    search_page::SearchPage,
    session_list::{SessionList, SessionListEvent},
};

const MAX_ENTRIES: usize = 100_000;

/// The files, as a list and as a tree, and whether the listing hit the
/// entry limit.
type Scan = (Arc<[PathBuf]>, Vec<vortex_project::TreeNode>, bool);

pub enum ProjectEvent {
    /// Its conversations were listed, or could not be.
    SessionsListed,
    /// A file picked on the Git page, to be shown in the editor.
    OpenFile(PathBuf),
    /// The Git page asked for the commit settings.
    CommitSettings,
}

pub struct Project {
    root: PathBuf,
    sessions: Entity<SessionList>,
    repo: Entity<RepoStatus>,
    files_page: Entity<FilesPage>,
    git_page: Entity<GitPage>,
    search_page: Entity<SearchPage>,
    files: Arc<[PathBuf]>,
    /// Lists the files; replaced by the next scan, so a slow one cannot
    /// land after it.
    scan: Option<Task<()>>,
    _subscriptions: [Subscription; 6],
}

impl EventEmitter<ProjectEvent> for Project {}

impl Project {
    pub fn new(root: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let sessions = cx.new(|_| SessionList::default());
        let repo = cx.new(|_| RepoStatus::new(root.clone()));
        let editor = cx.new(EditorPane::new);
        let aside = cx.new(EditorPane::new);
        let panes = [editor.clone(), aside.clone()];
        let files_page = cx.new(|cx| FilesPage::new(root.clone(), panes, repo.clone(), cx));
        let git_page = cx.new(|cx| GitPage::new(repo.clone(), window, cx));
        let search_page = cx.new(|cx| SearchPage::new(root.clone(), editor.clone(), window, cx));
        let subscriptions = [
            cx.subscribe(&sessions, |_, _, event, cx| match event {
                SessionListEvent::Loaded => cx.emit(ProjectEvent::SessionsListed),
            }),
            cx.subscribe(&editor, |this, _, _: &Saved, cx| {
                this.repo.update(cx, RepoStatus::refresh);
            }),
            cx.subscribe(&aside, |this, _, _: &Saved, cx| {
                this.repo.update(cx, RepoStatus::refresh);
            }),
            // Read together: what a file is compared with may have changed.
            cx.subscribe_in(&repo, window, |this, _, _: &StatusRead, window, cx| {
                for editor in this.editors(cx) {
                    editor.update(cx, |editor, cx| editor.refresh(window, cx));
                }
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
                GitPageEvent::CommitSettings => cx.emit(ProjectEvent::CommitSettings),
            }),
        ];
        Self {
            root,
            sessions,
            repo,
            files_page,
            git_page,
            search_page,
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

    /// What git says is changed in the project, read again as files change.
    pub fn repo(&self) -> &Entity<RepoStatus> {
        &self.repo
    }

    pub fn sessions(&self) -> &Entity<SessionList> {
        &self.sessions
    }

    /// The editor pane the editor's commands go to: the one last clicked.
    pub fn active_editor(&self, cx: &gpui_kit::App) -> Entity<EditorPane> {
        self.files_page.read(cx).active_editor(cx).clone()
    }

    /// Both editor panes of the Files page.
    pub fn editors(&self, cx: &gpui_kit::App) -> [Entity<EditorPane>; 2] {
        self.files_page.read(cx).editors()
    }

    pub fn files_page(&self) -> &Entity<FilesPage> {
        &self.files_page
    }

    pub fn git_page(&self) -> &Entity<GitPage> {
        &self.git_page
    }

    pub fn search_page(&self) -> &Entity<SearchPage> {
        &self.search_page
    }

    pub fn files(&self) -> Arc<[PathBuf]> {
        self.files.clone()
    }

    /// Lists the conversations every harness stored for the folder, as one
    /// list.
    pub fn reload_sessions(&self, backends: &vortex_chat::Backends, cx: &mut Context<Self>) {
        let listings = backends
            .iter()
            .map(|backend| backend.past_sessions(&self.root))
            .collect();
        let listing = crate::session_list::merged(listings);
        self.sessions
            .update(cx, |sessions, cx| sessions.reload(listing, cx));
    }

    /// Reads the files and git status again: agents and other programs
    /// change both behind Vortex's back.
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
            vortex_project::list_files(&root, &options).map(|list| {
                let files = list.files().map(Path::to_path_buf).collect();
                (files, vortex_project::build_tree(&list), list.truncated)
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
                self.files = files.clone();
                self.search_page.update(cx, |page, _| page.set_files(files));
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
        self.files_page
            .update(cx, |files, cx| files.open_in_editor(path, None, window, cx));
    }
}

pub fn folder_name(root: &Path) -> String {
    let name = root.file_name().unwrap_or(root.as_os_str());
    name.to_string_lossy().into_owned()
}
