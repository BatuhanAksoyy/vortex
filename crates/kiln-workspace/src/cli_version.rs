//! The Claude Code version in the status bar: green while it is the newest,
//! yellow once a newer one is out. Its panel updates it with the command
//! the CLI itself suggests, in the background, and restarts the open chats
//! still running an older one, which resume their conversations on it.

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use gpui_kit::prelude::*;
use gpui_kit::{
    Anchor, App, AsyncApp, Context, EventEmitter, FontWeight, Hsla, SharedString, Task,
    TestSupportExt, WeakEntity, Window,
    component::{
        ActiveTheme, Disableable as _, Sizable as _,
        button::{Button, ButtonVariants as _},
        h_flex,
        popover::Popover,
        spinner::Spinner,
        v_flex,
    },
    div, px,
};
use kiln_agent_claude::{Install, Method, is_newer};
use kiln_chat::scale::{ScaledButton as _, ScaledText as _};

/// As often as the CLI's own auto-updater looks.
const CHECK_EVERY: Duration = Duration::from_hours(1);
const PANEL_WIDTH: f32 = 300.;

pub struct CliVersion {
    program: PathBuf,
    install: Option<Install>,
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
}

impl EventEmitter<CliVersionEvent> for CliVersion {}

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
    install: Option<Install>,
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
    /// Reads the version of `program` now and every hour.
    pub fn new(program: PathBuf, cx: &mut Context<Self>) -> Self {
        Self {
            program,
            install: None,
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

    fn restart_chats(_: &mut Self, cx: &mut Context<Self>) {
        cx.emit(CliVersionEvent::RestartChats);
    }

    fn check(&self, cx: &Context<Self>) -> Task<Checked> {
        let program = self.program.clone();
        cx.background_spawn(async move { checked(&program) })
    }

    fn show(&mut self, checked: Checked, cx: &mut Context<Self>) {
        match checked {
            Ok((install, latest)) => {
                self.problem = latest.as_ref().err().map(|error| format!("{error:#}"));
                self.latest = latest.ok();
                self.install = Some(install);
            }
            Err(error) => self.problem = Some(format!("{error:#}")),
        }
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
        self.update = Update::Running;
        let run = cx.background_spawn(async move { install.update() });
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

type Checked = anyhow::Result<(Install, anyhow::Result<String>)>;

fn checked(program: &Path) -> Checked {
    let install = Install::find(program)?;
    let latest = install.latest();
    Ok((install, latest))
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
            install: self.install.clone(),
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
            .trigger(trigger(&shown, cx))
            .open(self.open)
            .on_open_change(move |open, _, cx| {
                let _ = opened.update(cx, |this, cx| this.set_open(*open, cx));
            })
            .content(move |_, _, cx| panel(&shown, &view, cx))
            .into_any_element()
    }
}

fn trigger(shown: &Shown, cx: &App) -> Button {
    let theme = cx.theme();
    let (dot, tooltip): (Hsla, SharedString) = match (&shown.install, &shown.latest) {
        (Some(_), Some(latest)) if shown.outdated() => (
            theme.warning,
            format!("Claude Code {latest} is available").into(),
        ),
        (Some(_), Some(_)) => (theme.success, "Claude Code is up to date".into()),
        _ => (theme.muted_foreground, "Claude Code".into()),
    };
    let label = shown.install.as_ref().map_or_else(
        || "Claude Code".to_owned(),
        |install| format!("Claude Code {}", install.version),
    );
    Button::new("cli-version")
        .ghost()
        .small()
        .px_1p5()
        .accessibility_label(label.clone())
        .text_color(theme.muted_foreground)
        .tooltip(tooltip)
        .child(
            h_flex()
                .gap_1p5()
                .child(div().size_2().rounded_full().bg(dot))
                .child(div().text_caption().child(label)),
        )
}

fn panel(shown: &Shown, view: &WeakEntity<CliVersion>, cx: &App) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let installed = shown.install.as_ref();
    let fact = |name: &'static str, value: String| {
        h_flex()
            .gap_2()
            .child(div().w_24().text_color(theme.muted_foreground).child(name))
            .child(div().min_w_0().truncate().child(value))
    };
    v_flex()
        .id("cli-version-panel")
        .test_support()
        .w(px(PANEL_WIDTH))
        .gap_3()
        // The status bar's small type would carry over into the panel.
        .text_sm()
        .text_color(theme.popover_foreground)
        .child(div().font_weight(FontWeight::SEMIBOLD).child("Claude Code"))
        .child(
            v_flex()
                .gap_1()
                .children(installed.map(|it| fact("Installed", it.version.clone())))
                .child(fact(
                    "Latest",
                    shown.latest.clone().unwrap_or_else(|| "Unknown".into()),
                ))
                .children(installed.map(|it| fact("Installed with", method_name(&it.method)))),
        )
        .child(status(shown, view, cx))
        .children(restart_offer(shown, view, cx))
        .child(
            h_flex().justify_end().child(
                Button::new("cli-version-check")
                    .ghost()
                    .control()
                    .label("Check again")
                    .disabled(matches!(shown.update, Update::Running))
                    .on_click(listener(view, CliVersion::check_again)),
            ),
        )
}

/// Where the panel says what can be done, or what happened.
fn status(shown: &Shown, view: &WeakEntity<CliVersion>, cx: &App) -> gpui_kit::AnyElement {
    let theme = cx.theme();
    let command = shown.install.as_ref().and_then(Install::update_command);
    let note = |text: String, color: Hsla| {
        div()
            .id("cli-version-status")
            .test_support()
            .text_sm()
            .text_color(color)
            .child(text)
            .into_any_element()
    };
    match (&shown.update, command) {
        (Update::Running, command) => h_flex()
            .id("cli-version-status")
            .test_support()
            .gap_2()
            .text_sm()
            .child(Spinner::new().small())
            .child(format!("Running {}…", command.unwrap_or_default()))
            .into_any_element(),
        (Update::Done, _) => note("Updated.".into(), theme.success),
        (Update::Failed(error), _) => note(error.clone(), theme.danger),
        (Update::Idle, Some(command)) if shown.outdated() => v_flex()
            .gap_1()
            .child(
                Button::new("cli-version-update")
                    .primary()
                    .control()
                    .label("Update Claude Code")
                    .on_click(listener(view, CliVersion::start_update)),
            )
            .child(
                div()
                    .text_caption()
                    .text_color(theme.muted_foreground)
                    .child(format!("Runs {command}")),
            )
            .into_any_element(),
        (Update::Idle, None) if shown.outdated() => note(
            "Update it the way you installed it.".into(),
            theme.muted_foreground,
        ),
        (Update::Idle, _) => match &shown.problem {
            Some(problem) => note(problem.clone(), theme.muted_foreground),
            None => note("Up to date.".into(), theme.muted_foreground),
        },
    }
}

/// Open chats on an older version restart on the installed one, each
/// resuming its conversation; a chat in a turn restarts when it ends.
fn restart_offer(
    shown: &Shown,
    view: &WeakEntity<CliVersion>,
    cx: &App,
) -> Option<gpui_kit::AnyElement> {
    if shown.behind == 0 || matches!(shown.update, Update::Running) {
        return None;
    }
    let chats = match shown.behind {
        1 => "1 open chat runs".to_owned(),
        n => format!("{n} open chats run"),
    };
    let element = v_flex()
        .gap_1()
        .child(
            div()
                .text_color(cx.theme().muted_foreground)
                .child(format!("{chats} an older version.")),
        )
        .child(
            Button::new("cli-version-restart")
                .outline()
                .control()
                .label("Restart Chats on This Version")
                .tooltip("Each chat resumes its conversation; a running turn finishes first")
                .on_click(listener(view, CliVersion::restart_chats)),
        )
        .into_any_element();
    Some(element)
}

fn listener(
    view: &WeakEntity<CliVersion>,
    act: fn(&mut CliVersion, &mut Context<CliVersion>),
) -> impl Fn(&gpui_kit::ClickEvent, &mut Window, &mut App) + 'static {
    let view = view.clone();
    move |_, _, cx| {
        let _ = view.update(cx, act);
    }
}

fn method_name(method: &Method) -> String {
    match method {
        Method::Homebrew { cask, .. } => format!("Homebrew ({cask})"),
        Method::Native => "Native installer".to_owned(),
        Method::Npm => "npm".to_owned(),
        Method::Unknown => "Unknown".to_owned(),
    }
}
