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
        h_flex,
        notification::Notification,
        status_bar::StatusBar,
        v_flex,
    },
};
use kiln_accounts::Accounts;
use kiln_agent::AgentBackend;
use kiln_chat::{ProcessMeter, ProcessMeterEvent, Processes, ProcessesDialog};

use crate::{
    chat_panel::ChatPanel,
    commands::ActiveWorkspace,
    config,
    left_panel::LeftPanel,
    saved_layout::{LayoutFile, Pane},
    session_list::SessionList,
};

mod actions;
mod activity_bar;
mod chats;
mod layout;
mod pages;
mod processes;

/// The project's files, as a list and as a tree, and whether the listing
/// hit the entry limit.
type Scan = (Arc<[PathBuf]>, Vec<kiln_project::TreeNode>, bool);

const MAX_PROJECT_ENTRIES: usize = 100_000;
const CONFIG_DEBOUNCE: Duration = Duration::from_millis(150);

pub struct Workspace {
    root: PathBuf,
    /// Starts the agent behind the chat; injected so tests run no real CLI.
    backend: Rc<dyn AgentBackend>,
    accounts: Entity<Accounts>,
    /// The project's stored conversations, which each chat's conversations
    /// page lists.
    sessions: Entity<SessionList>,
    /// What every agent's processes use, in the status bar.
    process_meter: Entity<ProcessMeter>,
    /// What the activity bar shows over the whole window.
    page: pages::Page,
    left_panel: LeftPanel,
    /// Where the left panel's size is kept.
    left_panel_file: Option<PathBuf>,
    left_panel_save: Option<Task<()>>,
    pages: pages::ProjectPages,
    /// The chats, as tabs in groups the user can split.
    dock: Entity<DockArea>,
    chats: Vec<chats::ChatTab>,
    /// The chat the user last worked in.
    active_chat: Option<Entity<ChatPanel>>,
    /// Where each project's chat layout is kept; `None` keeps none.
    layouts: Option<LayoutFile>,
    /// The kept layout, until the session list has loaded.
    pending_layout: Option<Pane>,
    files: Arc<[PathBuf]>,
    /// Lists the project's files.
    scan: Option<Task<()>>,
    /// The last reading of the agents' processes, which chats show.
    processes: Entity<Processes>,
    /// Reads the agents' processes while the window is open.
    _process_watch: Task<()>,
    /// Every agent's processes over the whole window, while open.
    processes_page: Option<(Entity<ProcessesDialog>, Subscription)>,
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
        let processes = Processes::shared(cx);
        let meter = cx.new(|cx| ProcessMeter::all(processes.clone(), cx));
        let sessions = cx.new(|_| SessionList::default());
        let pages = pages::ProjectPages::new(&root, window, cx);
        let dock = chats::new_dock(window, cx);
        let subscriptions = vec![
            cx.subscribe_in(&meter, window, |this, _, event, window, cx| match event {
                ProcessMeterEvent::Open => this.open_processes(window, cx),
            }),
            cx.subscribe_in(&sessions, window, Self::on_session_list),
            cx.subscribe(&dock, |this, _, event: &DockEvent, cx| {
                if matches!(event, DockEvent::LayoutChanged) {
                    this.save_layout(cx);
                }
            }),
            cx.observe_window_activation(window, |this, window, cx| {
                if window.is_window_active() {
                    this.refresh_project(window, cx);
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
            process_meter: meter,
            page: pages::Page::default(),
            left_panel: LeftPanel::default(),
            left_panel_file: None,
            left_panel_save: None,
            pages,
            dock,
            chats: Vec::new(),
            active_chat: None,
            layouts: None,
            pending_layout: None,
            files: Arc::from([]),
            scan: None,
            processes,
            _process_watch: Self::watch_processes(cx),
            processes_page: None,
            focus_handle: cx.focus_handle(),
            config_watch: None,
            _subscriptions: subscriptions,
        }
    }

    /// Keeps each project's chat layout in the file at `path`, to reopen it
    /// on the next start.
    #[must_use]
    pub fn keeping_layouts_in(mut self, path: PathBuf) -> Self {
        let left_panel = path.with_file_name("left-panel.json");
        self.left_panel = LeftPanel::load(&left_panel);
        self.left_panel_file = Some(left_panel);
        self.layouts = Some(LayoutFile::new(path));
        self
    }

    /// The file shown in the editor pane, if any.
    pub fn active_file(&self, cx: &gpui_kit::App) -> Option<PathBuf> {
        self.pages
            .editor
            .read(cx)
            .active_path(cx)
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
        self.refresh_project(window, cx);
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
        self.pages
            .editor
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
        let active_file = self.pages.editor.read(cx).active_path(cx).map(|path| {
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
            .child(
                TitleBar::new().child(
                    h_flex()
                        .gap_2()
                        .child(self.render_panel_toggle(cx))
                        .child(project_name(&self.root)),
                ),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .children(self.render_activity_bar(window, cx))
                    .child(
                        gpui_kit::div()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .child(self.render_page()),
                    ),
            )
            .children(self.render_processes(window, cx))
            .child(
                StatusBar::new()
                    .left(self.process_meter.clone())
                    .left(self.root.display().to_string())
                    .child(active_file.unwrap_or_default()),
            )
    }
}
