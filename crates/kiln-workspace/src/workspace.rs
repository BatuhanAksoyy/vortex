use std::{
    path::{Path, PathBuf},
    rc::Rc,
    time::Duration,
};

use futures::StreamExt as _;
use gpui_kit::prelude::*;
use gpui_kit::{
    BackgroundExecutor, Context, Entity, EntityId, FocusHandle, Focusable, Subscription, Task,
    Window,
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
    browser::Browser,
    chat_panel::ChatPanel,
    cli_version::{CliVersion, CliVersionEvent},
    config,
    project::{Project, folder_name},
    project_picker::ProjectPicker,
    recent_projects::RecentProjects,
    saved_layout::{LayoutFile, Pane},
    side_panel::SidePanel,
};

mod actions;
mod activity_bar;
mod agent_tools;
mod browsers;
mod chats;
mod cli_update;
mod layout;
mod processes;
mod projects;
mod side;
mod switcher;
mod tabs;
mod worktrees;

const CONFIG_DEBOUNCE: Duration = Duration::from_millis(150);

pub struct Workspace {
    /// The folder Kiln was started in, until its session opens.
    first_folder: Option<PathBuf>,
    /// Every folder a session was opened in.
    projects: Vec<(Entity<Project>, Subscription)>,
    /// The folder the side panel shows: the active session's.
    shown_root: Option<PathBuf>,
    recent: RecentProjects,
    /// The folders to choose from while no session is open.
    start_page: Option<(Entity<ProjectPicker>, Subscription)>,
    /// Starts the agent behind the chat; injected so tests run no real CLI.
    backend: Rc<dyn AgentBackend>,
    accounts: Entity<Accounts>,
    /// What every agent's processes use, in the status bar.
    process_meter: Entity<ProcessMeter>,
    /// The Claude Code version, in the status bar; none in tests, which run
    /// no real CLI.
    cli_version: Option<(Entity<CliVersion>, Subscription)>,
    side_panel: SidePanel,
    /// The sessions' browsers, by chat.
    browsers: Vec<(EntityId, Entity<Browser>)>,
    /// Where Kiln keeps its data: the browsers' profile, and what chats keep
    /// of a conversation that the agent does not store; `None` keeps none.
    data_dir: Option<PathBuf>,
    agent_tools: Option<agent_tools::AgentTools>,
    /// Whether the side panel fills the window while open; not kept across restarts.
    side_maximized: bool,
    /// Where the side panel's state is kept.
    side_panel_file: Option<PathBuf>,
    side_panel_save: Option<Task<()>>,
    /// The chats, as tabs in groups the user can split.
    dock: Entity<DockArea>,
    chats: Vec<chats::ChatTab>,
    /// The chat the user last worked in.
    active_chat: Option<Entity<ChatPanel>>,
    /// Where the chat layout is kept; `None` keeps none.
    layouts: Option<LayoutFile>,
    /// The kept layout, until its conversations are listed.
    pending_layout: Option<Pane>,
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
    /// A window that opens a session in `folder`, if given.
    pub fn new(
        folder: Option<PathBuf>,
        backend: Rc<dyn AgentBackend>,
        accounts: Entity<Accounts>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        crate::windows::track(window, cx);
        let processes = Processes::shared(cx);
        let meter = cx.new(|cx| ProcessMeter::all(processes.clone(), cx));
        let dock = chats::new_dock(window, cx);
        let subscriptions = vec![
            cx.subscribe_in(&meter, window, |this, _, event, window, cx| match event {
                ProcessMeterEvent::Open => this.open_processes(window, cx),
            }),
            cx.subscribe_in(&dock, window, |this, _, event, window, cx| {
                if matches!(event, DockEvent::LayoutChanged) {
                    this.follow_dock(window, cx);
                    this.save_layout(cx);
                }
            }),
            cx.observe_window_activation(window, |this, window, cx| {
                if window.is_window_active() {
                    crate::windows::make_active(window, cx);
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
        cx.defer_in(window, Self::start_chats);
        Self {
            first_folder: folder,
            projects: Vec::new(),
            shown_root: None,
            recent: RecentProjects::default(),
            start_page: None,
            backend,
            accounts,
            process_meter: meter,
            cli_version: None,
            side_panel: SidePanel::default(),
            browsers: Vec::new(),
            data_dir: None,
            agent_tools: None,
            side_maximized: false,
            side_panel_file: None,
            side_panel_save: None,
            dock,
            chats: Vec::new(),
            active_chat: None,
            layouts: None,
            pending_layout: None,
            processes,
            _process_watch: Self::watch_processes(cx),
            processes_page: None,
            focus_handle: cx.focus_handle(),
            config_watch: None,
            _subscriptions: subscriptions,
        }
    }

    /// Keeps the chat layout in the file at `path`, to reopen it on the next
    /// start, and the recent projects, side panel and browser data beside it.
    #[must_use]
    pub fn keeping_layouts_in(self, path: PathBuf) -> Self {
        let mut this = self.keeping_data_beside(&path);
        let projects = path.with_file_name("projects.json");
        let layouts = LayoutFile::new(path);
        this.recent = RecentProjects::load(projects, || layouts.projects());
        this.layouts = Some(layouts);
        this
    }

    /// For a further window: the recent projects, side panel and browser
    /// data beside `path`, but not the layout there, which is the first
    /// window's.
    #[must_use]
    pub fn keeping_data_beside(mut self, path: &Path) -> Self {
        let side_panel = path.with_file_name("side-panel.json");
        self.side_panel = SidePanel::load(&side_panel);
        self.side_panel_file = Some(side_panel);
        self.data_dir = path.parent().map(Path::to_path_buf);
        self.agent_tools = path.parent().and_then(agent_tools::AgentTools::start);
        self.recent = RecentProjects::load(path.with_file_name("projects.json"), Vec::new);
        self
    }

    /// Shows `version` in the status bar, whose panel restarts the chats
    /// left on an older one.
    #[must_use]
    pub fn showing_cli_version(
        mut self,
        version: Entity<CliVersion>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let workspace = cx.weak_entity();
        version.update(cx, |version, _| {
            version.count_chats_behind(move |installed, cx| {
                let workspace = workspace.upgrade();
                workspace.map_or(0, |it| it.read(cx).chats_behind(installed, cx).len())
            });
        });
        let restart = cx.subscribe_in(&version, window, |this, _, event, window, cx| match event {
            CliVersionEvent::RestartChats => this.restart_chats_behind(window, cx),
        });
        self.cli_version = Some((version, restart));
        self
    }

    /// The file shown in the active session's editor, if any.
    pub fn active_file(&self, cx: &gpui_kit::App) -> Option<PathBuf> {
        let editor = self.project(cx)?.read(cx).editor();
        editor.read(cx).active_path(cx).map(Path::to_path_buf)
    }

    /// Follows the active session to its project.
    fn load_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let title = match &self.shown_root {
            Some(root) => format!("{} — Kiln", folder_name(root)),
            None => "Kiln".to_owned(),
        };
        window.set_window_title(&title);
        self.reload_config(window, cx);
        let watch = Self::watch_config(window, cx);
        let _previous = self.config_watch.replace(watch);
        if let Some(project) = self.project(cx).cloned() {
            // Also while the panel is closed: going to a file needs them.
            project.update(cx, |project, cx| project.scan_files(window, cx));
        }
        self.refresh_project(window, cx);
    }

    fn reload_config(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for message in config::reload(self.shown_root.as_deref(), cx) {
            window.push_notification(Notification::warning(message), cx);
        }
        for (project, _) in &self.projects {
            let editor = project.read(cx).editor().clone();
            editor.update(cx, |pane, cx| pane.apply_settings(window, cx));
        }
        for tab in &self.chats {
            tab.panel.update(cx, ChatPanel::settings_changed);
        }
        cx.notify();
    }

    /// Reloads configuration whenever a settings or keybindings file changes.
    fn watch_config(window: &mut Window, cx: &mut Context<Self>) -> Task<()> {
        let (sender, mut receiver) = futures::channel::mpsc::unbounded::<()>();
        let files = config::watched_files(cx);
        let watcher = kiln_settings::FileWatcher::new(&files, move || {
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

impl Focusable for Workspace {
    fn focus_handle(&self, _: &gpui_kit::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let root = self.shown_root.clone().unwrap_or_default();
        let active_file = self.active_file(cx).map(|path| {
            let relative = path.strip_prefix(&root).unwrap_or(&path);
            relative.display().to_string()
        });
        let name = match &self.shown_root {
            Some(root) => folder_name(root),
            None => "Kiln".to_owned(),
        };
        v_flex()
            .id("workspace")
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                TitleBar::new().child(
                    h_flex()
                        .w_full()
                        .pr_2()
                        .gap_2()
                        .children(self.render_panel_toggle(cx))
                        .child(
                            // A row, so the menu's button keeps to the left.
                            h_flex()
                                .flex_1()
                                .min_w_0()
                                .justify_start()
                                .child(self.render_switcher(name, cx)),
                        )
                        .children(self.render_browser_toggle(cx)),
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
                            .child(self.render_main(window, cx)),
                    ),
            )
            .children(self.render_processes(window, cx))
            .child(
                StatusBar::new()
                    .left(self.process_meter.clone())
                    .left(root.display().to_string())
                    .right(active_file.unwrap_or_default())
                    .when_some(self.cli_version.as_ref(), |bar, (version, _)| {
                        bar.right(version.clone())
                    }),
            )
    }
}
