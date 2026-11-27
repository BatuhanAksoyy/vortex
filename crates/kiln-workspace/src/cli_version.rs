//! An agent CLI's version in the status bar: green while it is the newest,
//! yellow once a newer one is out, and a missing CLI says so. Its panel
//! installs or updates it in the background, with Homebrew where it is or
//! the command the CLI itself suggests, and restarts the open chats still
//! running an older one, which resume their conversations on it.

mod panel;

use std::{sync::Arc, time::Duration};

use gpui_kit::prelude::*;
use gpui_kit::{Anchor, App, AsyncApp, Context, EventEmitter, Task, WeakEntity, Window};
use gpui_kit::{component::popover::Popover, div};
use kiln_agent::{Cli, Install, is_newer};
use kiln_chat::Setup;

/// As often as the CLI's own auto-updater looks.
const CHECK_EVERY: Duration = Duration::from_hours(1);

pub struct CliVersion {
    cli: Arc<dyn Cli>,
    install: Option<Install>,
    /// The CLI is not on `PATH`.
    missing: bool,
    /// What installs it, as the user would type it.
    install_command: Option<String>,
    latest: Option<String>,
    /// Why the version or the newest release could not be read.
    problem: Option<String>,
    update: Update,
    open: bool,
    _checks: Task<()>,
    updating: Option<Task<()>>,
    /// How many open chats run a version other than the one given.
    behind: Option<CountBehind>,
}

type CountBehind = Box<dyn Fn(&str, &App) -> usize>;

pub enum CliVersionEvent {
    /// Restart the chats that run another version than the installed one.
    RestartChats,
    /// Whether the CLI is missing or being installed; `None` once it is
    /// there. Sent after every check.
    Setup(Option<Setup>),
}

impl EventEmitter<CliVersionEvent> for CliVersion {}

/// An installation or update under way, or how the last one went.
#[derive(Clone)]
enum Update {
    Idle,
    Running,
    Done,
    Failed(String),
}

/// What the panel shows, copied out of the view each render.
#[derive(Clone)]
struct Shown {
    title: &'static str,
    install: Option<Install>,
    missing: bool,
    install_command: Option<String>,
    latest: Option<String>,
    problem: Option<String>,
    update: Update,
    /// Open chats on another version than the installed one.
    behind: usize,
}

impl Shown {
    fn outdated(&self) -> bool {
        let (Some(install), Some(latest)) = (&self.install, &self.latest) else {
            return false;
        };
        is_newer(latest, &install.version)
    }
}

impl CliVersion {
    /// Reads the version of `cli` now and every hour.
    pub fn new(cli: Arc<dyn Cli>, cx: &mut Context<Self>) -> Self {
        Self {
            cli,
            install: None,
            missing: false,
            install_command: None,
            latest: None,
            problem: None,
            update: Update::Idle,
            open: false,
            _checks: cx.spawn(async |this, cx| check_hourly(this, cx).await),
            updating: None,
            behind: None,
        }
    }

    /// Counts the open chats on a version other than the installed one,
    /// which the panel offers to restart.
    pub fn count_chats_behind(&mut self, count: impl Fn(&str, &App) -> usize + 'static) {
        self.behind = Some(Box::new(count));
    }

    /// The version installed, as last read.
    #[must_use]
    pub fn installed(&self) -> Option<String> {
        self.install.as_ref().map(|install| install.version.clone())
    }

    /// Installs the missing CLI, with the panel open to show how it goes.
    pub fn start_install(&mut self, cx: &mut Context<Self>) {
        if !self.missing || matches!(self.update, Update::Running) {
            return;
        }
        self.open = true;
        let cli = self.cli.clone();
        self.run_then_check(move || kiln_agent::install(&*cli), cx);
        cx.emit(CliVersionEvent::Setup(Some(Setup::Installing)));
    }

    fn restart_chats(_: &mut Self, cx: &mut Context<Self>) {
        cx.emit(CliVersionEvent::RestartChats);
    }

    fn check(&self, cx: &Context<Self>) -> Task<Checked> {
        let cli = self.cli.clone();
        cx.background_spawn(async move { checked(cli) })
    }

    fn show(&mut self, checked: Checked, cx: &mut Context<Self>) {
        self.missing = !checked.installed;
        self.install_command = checked.install_command;
        match checked.found {
            Ok((install, latest)) => {
                self.problem = latest.as_ref().err().map(|error| format!("{error:#}"));
                self.latest = latest.ok();
                self.install = Some(install);
            }
            Err(error) => {
                self.problem = Some(format!("{error:#}"));
                if self.missing {
                    self.install = None;
                }
            }
        }
        let setup = self.missing.then(|| match &self.update {
            Update::Running => Setup::Installing,
            Update::Failed(why) => Setup::Failed(why.clone()),
            Update::Idle | Update::Done => Setup::Missing,
        });
        cx.emit(CliVersionEvent::Setup(setup));
        cx.notify();
    }

    fn check_again(&mut self, cx: &mut Context<Self>) {
        let check = self.check(cx);
        cx.spawn(async move |this, cx| {
            let checked = check.await;
            let _ = this.update(cx, |this, cx| this.show(checked, cx));
        })
        .detach();
    }

    fn start_update(&mut self, cx: &mut Context<Self>) {
        let Some(install) = self.install.clone() else {
            return;
        };
        self.run_then_check(move || install.update(), cx);
    }

    /// Runs `job` in the background, then reads the install again.
    fn run_then_check(
        &mut self,
        job: impl FnOnce() -> anyhow::Result<()> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        self.update = Update::Running;
        let run = cx.background_spawn(async move { job() });
        self.updating = Some(cx.spawn(async move |this, cx| {
            let result = run.await;
            let Ok(check) = this.update(cx, |this, cx| this.check(cx)) else {
                return;
            };
            let checked = check.await;
            let _ = this.update(cx, |this, cx| {
                this.update = match result {
                    Ok(()) => Update::Done,
                    Err(error) => Update::Failed(format!("{error:#}")),
                };
                this.show(checked, cx);
            });
        }));
        cx.notify();
    }

    fn set_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.open = open;
        if !open && matches!(self.update, Update::Done | Update::Failed(_)) {
            self.update = Update::Idle;
        }
        cx.notify();
    }
}

struct Checked {
    installed: bool,
    install_command: Option<String>,
    found: anyhow::Result<(Install, anyhow::Result<String>)>,
}

fn checked(cli: Arc<dyn Cli>) -> Checked {
    let installed = kiln_agent::is_installed(&*cli);
    let install_command = kiln_agent::install_command(&*cli);
    let found = Install::find(cli).map(|install| {
        let latest = install.latest();
        (install, latest)
    });
    Checked {
        installed,
        install_command,
        found,
    }
}

async fn check_hourly(this: WeakEntity<CliVersion>, cx: &mut AsyncApp) {
    loop {
        let Ok(check) = this.update(cx, |this, cx| this.check(cx)) else {
            return;
        };
        let checked = check.await;
        if this.update(cx, |this, cx| this.show(checked, cx)).is_err() {
            return;
        }
        cx.background_executor().timer(CHECK_EVERY).await;
    }
}

impl Render for CliVersion {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let shown = Shown {
            title: self.cli.title(),
            install: self.install.clone(),
            missing: self.missing,
            install_command: self.install_command.clone(),
            latest: self.latest.clone(),
            problem: self.problem.clone(),
            update: self.update.clone(),
            behind: match (&self.install, &self.behind) {
                (Some(install), Some(count)) => count(&install.version, cx),
                _ => 0,
            },
        };
        if shown.install.is_none() && shown.problem.is_none() {
            return div().into_any_element();
        }
        let view = cx.entity().downgrade();
        let opened = view.clone();
        Popover::new("cli-version-popover")
            .anchor(Anchor::BottomRight)
            .trigger(panel::trigger(&shown, cx))
            .open(self.open)
            .on_open_change(move |open, _, cx| {
                let _ = opened.update(cx, |this, cx| this.set_open(*open, cx));
            })
            .content(move |_, _, cx| panel::panel(&shown, &view, cx))
            .into_any_element()
    }
}
