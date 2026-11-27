//! Vortex's own updates for the whole app: looking on GitHub for a newer
//! release now and then, putting it in place of the running app, and
//! starting it. Each window's status bar offers what there is to do.

use std::{path::PathBuf, time::Duration};

use gpui_kit::{App, AppContext as _, Context, Entity, EntityId, EventEmitter, Global, Task};
use vortex_update::Release;

use crate::config;

/// Once the window is up, so the check does not slow the start.
const FIRST_CHECK: Duration = Duration::from_secs(10);
const CHECK_EVERY: Duration = Duration::from_hours(6);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateState {
    Idle,
    Checking,
    /// A newer release to install.
    Available(Release),
    Installing(Release),
    /// In place; it runs once Vortex starts again.
    Ready(Release, PathBuf),
    Failed(String),
}

/// What a check the user asked for in window `by` found, to tell them
/// there.
pub enum UpdatesEvent {
    Checked {
        by: EntityId,
        found: Result<Option<Release>, String>,
    },
}

pub struct Updates {
    state: UpdateState,
    checking: Task<()>,
    _watch: Task<()>,
}

struct Shared(Entity<Updates>);

impl Global for Shared {}

impl EventEmitter<UpdatesEvent> for Updates {}

/// Starts looking for updates, while the settings ask for it. Vortex calls
/// it once; without it, as in tests, nothing is ever asked of GitHub.
pub fn watch(cx: &mut App) {
    let updates = cx.new(|cx| Updates {
        state: UpdateState::Idle,
        checking: Task::ready(()),
        _watch: cx.spawn(async move |this, cx| {
            cx.background_executor().timer(FIRST_CHECK).await;
            loop {
                let wanted = cx.update(|cx| config::settings(cx).update.check);
                if wanted
                    && this
                        .update(cx, |this: &mut Updates, cx| this.check(None, cx))
                        .is_err()
                {
                    return;
                }
                cx.background_executor().timer(CHECK_EVERY).await;
            }
        }),
    });
    cx.set_global(Shared(updates));
}

/// The app's updates, once [`watch`] started them.
pub fn shared(cx: &App) -> Option<Entity<Updates>> {
    cx.try_global::<Shared>().map(|shared| shared.0.clone())
}

impl Updates {
    pub fn state(&self) -> &UpdateState {
        &self.state
    }

    /// Asks GitHub for the newest release; what it found is reported to
    /// the window `by`, if the user asked there.
    pub fn check(&mut self, by: Option<EntityId>, cx: &mut Context<Self>) {
        if !matches!(self.state, UpdateState::Idle | UpdateState::Failed(_)) {
            if let Some(by) = by {
                let found = Ok(self.offered());
                cx.emit(UpdatesEvent::Checked { by, found });
            }
            return;
        }
        self.state = UpdateState::Checking;
        cx.notify();
        let asked = cx.background_spawn(async { vortex_update::latest() });
        self.checking = cx.spawn(async move |this, cx| {
            let found = asked.await;
            let _ = this.update(cx, |this, cx| this.checked(found, by, cx));
        });
    }

    fn checked(
        &mut self,
        found: anyhow::Result<Release>,
        by: Option<EntityId>,
        cx: &mut Context<Self>,
    ) {
        let found = found.map(|release| release.is_newer().then_some(release));
        self.state = match &found {
            Ok(Some(release)) => UpdateState::Available(release.clone()),
            Ok(None) => UpdateState::Idle,
            Err(error) => {
                log::warn!("cannot look for a Vortex update: {error:#}");
                UpdateState::Failed(format!("{error:#}"))
            }
        };
        if let Some(by) = by {
            let found = found.map_err(|error| format!("{error:#}"));
            cx.emit(UpdatesEvent::Checked { by, found });
        }
        cx.notify();
    }

    /// The release on offer, found or being installed.
    fn offered(&self) -> Option<Release> {
        match &self.state {
            UpdateState::Available(release)
            | UpdateState::Installing(release)
            | UpdateState::Ready(release, _) => Some(release.clone()),
            UpdateState::Idle | UpdateState::Checking | UpdateState::Failed(_) => None,
        }
    }

    /// Puts the release on offer in place of the running app; one that does
    /// not run from an app bundle, or a release without one for this
    /// machine, gets its page opened instead.
    pub fn install(&mut self, cx: &mut Context<Self>) {
        let UpdateState::Available(release) = self.state.clone() else {
            return;
        };
        let bundle = vortex_update::running_bundle();
        let (Some(bundle), Some(url)) = (bundle, release.download.clone()) else {
            cx.open_url(&release.page);
            return;
        };
        self.state = UpdateState::Installing(release.clone());
        cx.notify();
        let installing = cx.background_spawn({
            let bundle = bundle.clone();
            async move { vortex_update::install(&url, &bundle) }
        });
        self.checking = cx.spawn(async move |this, cx| {
            let installed = installing.await;
            let _ = this.update(cx, |this, cx| {
                this.state = match installed {
                    Ok(()) => UpdateState::Ready(release, bundle),
                    Err(error) => UpdateState::Failed(format!("{error:#}")),
                };
                cx.notify();
            });
        });
    }

    /// Starts the installed release and quits this one.
    pub fn restart(&mut self, cx: &mut Context<Self>) {
        let UpdateState::Ready(_, bundle) = &self.state else {
            return;
        };
        match vortex_update::relaunch(bundle) {
            Ok(()) => cx.quit(),
            Err(error) => {
                self.state = UpdateState::Failed(format!("{error:#}"));
                cx.notify();
            }
        }
    }
}
