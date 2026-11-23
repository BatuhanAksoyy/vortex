use std::{
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use futures::StreamExt as _;
use gpui_kit::prelude::*;
use gpui_kit::{
    BackgroundExecutor, Context, Entity, FocusHandle, Focusable, Subscription, Task, Window,
    component::{
        ActiveTheme, TitleBar, WindowExt,
        dock::{DockArea, DockEvent},
        notification::Notification,
        resizable::{h_resizable, resizable_panel},
        status_bar::StatusBar,
        v_flex,
    },
};
use kiln_accounts::Accounts;
use kiln_agent::AgentBackend;
use kiln_project::ListOptions;

use crate::{
    chat_panel::ChatPanel,
    commands::ActiveWorkspace,
    config,
    editor_pane::EditorPane,
    saved_layout::{LayoutFile, Pane},
    session_list::SessionList,
};

mod actions;
mod chats;
mod layout;

/// The project's files, and whether the listing hit the entry limit.
type Scan = (Arc<[PathBuf]>, bool);

const MAX_PROJECT_ENTRIES: usize = 100_000;
const CONFIG_DEBOUNCE: Duration = Duration::from_millis(150);
const SIDEBAR_WIDTH_REMS: f32 = 18.0;
const EDITOR_WIDTH_REMS: f32 = 40.0;

pub struct Workspace {
    root: PathBuf,
    /// Starts the agent behind the chat; injected so tests run no real CLI.
    backend: Rc<dyn AgentBackend>,
    accounts: Entity<Accounts>,
    sessions: Entity<SessionList>,
    editor_pane: Entity<EditorPane>,
    /// The chats, as tabs in groups the user can split.
    dock: Entity<DockArea>,
    chats: Vec<chats::ChatTab>,
    /// The chat the user last worked in; the sidebar follows it.
    active_chat: Option<Entity<ChatPanel>>,
    /// Where each project's chat layout is kept; `None` keeps none.
    layouts: Option<LayoutFile>,
    /// The kept layout, until the session list has loaded.
    pending_layout: Option<Pane>,
    files: Arc<[PathBuf]>,
    sidebar_visible: bool,
    editor_visible: bool,
    focus_handle: FocusHandle,
    /// Dropping the task stops the watcher, so it is replaced, not detached,
    /// when another folder is opened.
    config_watch: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl Workspace {
    pub fn new(
        root: PathBuf,
        backend: Rc<dyn AgentBackend>,
        accounts: Entity<Accounts>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let workspace = cx.entity().downgrade();
        cx.set_global(ActiveWorkspace {
            window: window.window_handle(),
            workspace,
        });
        cx.on_release(|_, cx| cx.quit()).detach();
        let sessions = cx.new(|_| SessionList::default());
        let editor_pane = cx.new(EditorPane::new);
        let dock = chats::new_dock(window, cx);
        let subscriptions = vec![
            cx.subscribe_in(&sessions, window, Self::on_session_list),
            cx.subscribe(&dock, |this, _, event: &DockEvent, cx| {
                if matches!(event, DockEvent::LayoutChanged) {
                    this.save_layout(cx);
                }
            }),
            cx.observe_window_appearance(window, |_, window, cx| {
                if config::settings(cx).theme.mode == kiln_settings::ThemeMode::System {
                    gpui_kit::component::Theme::sync_system_appearance(Some(window), cx);
                }
            }),
        ];
        // Deferred: notifications need the window's Root, which exists only
        // after this constructor returns, and the chats wait for
        // `keeping_layouts_in`.
        cx.defer_in(window, Self::load_project);
        Self {
            root,
            backend,
            accounts,
            sessions,
            editor_pane,
            dock,
            chats: Vec::new(),
            active_chat: None,
            layouts: None,
            pending_layout: None,
            files: Arc::from([]),
            sidebar_visible: true,
            editor_visible: false,
            focus_handle: cx.focus_handle(),
            config_watch: None,
            _subscriptions: subscriptions,
        }
    }

    /// Keeps each project's chat layout in the file at `path`, to reopen it
    /// on the next start.
    #[must_use]
    pub fn keeping_layouts_in(mut self, path: PathBuf) -> Self {
        self.layouts = Some(LayoutFile::new(path));
        self
    }

    /// The file shown in the editor pane, if any.
    pub fn active_file(&self, cx: &gpui_kit::App) -> Option<PathBuf> {
        self.editor_pane
            .read(cx)
            .active_path()
            .map(Path::to_path_buf)
    }

    fn load_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.set_window_title(&format!("{} — Kiln", project_name(&self.root)));
        self.reload_config(window, cx);
        let watch = self.watch_config(window, cx);
        let _previous = self.config_watch.replace(watch);
        self.scan_files(window, cx);
        self.start_chats(window, cx);
        self.reload_sessions(cx);
    }

    fn reload_sessions(&mut self, cx: &mut Context<Self>) {
        let listing = self.backend.past_sessions(&self.root);
        self.sessions
            .update(cx, |sessions, cx| sessions.reload(listing, cx));
    }

    fn reload_config(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for message in config::reload(&self.root, cx) {
            window.push_notification(Notification::warning(message), cx);
        }
        self.editor_pane
            .update(cx, |pane, cx| pane.apply_settings(window, cx));
        cx.notify();
    }

    /// Reloads configuration whenever a settings or keybindings file changes.
    fn watch_config(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Task<()> {
        let (sender, mut receiver) = futures::channel::mpsc::unbounded::<()>();
        let watcher =
            kiln_settings::FileWatcher::new(&config::watched_files(&self.root), move || {
                let _ = sender.unbounded_send(());
            });
        let watcher = match watcher {
            Ok(watcher) => watcher,
            Err(error) => {
                let message = format!("Settings will not reload automatically: {error:#}");
                window.push_notification(Notification::warning(message), cx);
                return Task::ready(());
            }
        };
        cx.spawn_in(window, async move |this, cx| {
            let _watcher = watcher;
            while next_change(&mut receiver, cx.background_executor()).await
                && this.update_in(cx, Self::reload_config).is_ok()
            {}
        })
    }

    fn scan_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let root = self.root.clone();
        let options = ListOptions {
            exclude: config::settings(cx).files.exclude.clone(),
            limit: MAX_PROJECT_ENTRIES,
        };
        let scan = cx.background_spawn(async move {
            kiln_project::list_files(&root, &options).map(|list| {
                let files: Arc<[PathBuf]> = list.files().map(Path::to_path_buf).collect();
                (files, list.truncated)
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = scan.await;
            let _ = this.update_in(cx, |this, window, cx| this.apply_scan(result, window, cx));
        })
        .detach();
    }

    fn open_relative(&mut self, relative: &Path, window: &mut Window, cx: &mut Context<Self>) {
        self.open_absolute(self.root.join(relative), window, cx);
    }

    fn open_absolute(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.editor_visible = true;
        self.editor_pane
            .update(cx, |pane, cx| pane.open_path(path, window, cx));
        cx.notify();
    }

    fn apply_scan(
        &mut self,
        result: anyhow::Result<Scan>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok((files, truncated)) => {
                self.files = files;
                if truncated {
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

    fn render_body(&self, window: &Window, cx: &gpui_kit::App) -> impl IntoElement {
        let rem = window.rem_size();
        // With no file open the pane would only take room from the chat.
        let editor_shown = self.editor_visible && !self.editor_pane.read(cx).is_empty();
        h_resizable("workspace-panels")
            .when(self.sidebar_visible, |this| {
                this.child(
                    resizable_panel()
                        .size(rem * SIDEBAR_WIDTH_REMS)
                        .child(self.sessions.clone()),
                )
            })
            .child(resizable_panel().child(self.dock.clone()))
            .when(editor_shown, |this| {
                this.child(
                    resizable_panel()
                        .size(rem * EDITOR_WIDTH_REMS)
                        .child(self.editor_pane.clone()),
                )
            })
    }
}

/// Waits for a file change, then for the burst of follow-up events an editor
/// save produces to settle. Returns `false` once the watcher is gone.
async fn next_change(
    receiver: &mut futures::channel::mpsc::UnboundedReceiver<()>,
    executor: &BackgroundExecutor,
) -> bool {
    if receiver.next().await.is_none() {
        return false;
    }
    executor.timer(CONFIG_DEBOUNCE).await;
    while receiver.try_recv().is_ok() {}
    true
}

fn project_name(root: &Path) -> String {
    root.file_name().map_or_else(
        || root.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

impl Focusable for Workspace {
    fn focus_handle(&self, _: &gpui_kit::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active_file = self.editor_pane.read(cx).active_path().map(|path| {
            path.strip_prefix(&self.root)
                .unwrap_or(path)
                .display()
                .to_string()
        });
        v_flex()
            .id("workspace")
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(TitleBar::new().child(project_name(&self.root)))
            .child(
                gpui_kit::div()
                    .flex_1()
                    .min_h_0()
                    .child(self.render_body(window, cx)),
            )
            .child(
                StatusBar::new()
                    .left(self.root.display().to_string())
                    .child(active_file.unwrap_or_default()),
            )
    }
}
