//! Vortex's own updates for the whole app: looking on GitHub for a newer
//! release now and then, putting it in place of the running app, and
//! starting it. The gear of the window the user works in offers what there
//! is to do, in a panel beside it.

use std::{path::PathBuf, time::Duration};

use gpui_kit::{App, AppContext as _, Context, Entity, EntityId, Global, Task};
use vortex_update::{Release, Version};

use crate::config;

/// Once the window is up, so the check does not slow the start.
const FIRST_CHECK: Duration = Duration::from_secs(10);
/// A release is one small request away, and a Vortex left open for days
/// should hear of one the same day.
const CHECK_EVERY: Duration = Duration::from_hours(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateState {
    Idle,
    Checking,
    /// The last check found nothing newer.
    UpToDate,
    /// A newer release to install.
    Available(Release),
    Installing(Release),
    /// In place; it runs once Vortex starts again.
    Ready(Release, PathBuf),
    Failed(String),
}

pub struct Updates {
    state: UpdateState,
    /// The window whose gear shows the panel, by its workspace.
    shown_in: Option<EntityId>,
    /// The newest release the panel opened for on its own, so it does so
    /// once for each.
    announced: Option<Version>,
    checking: Task<()>,
    _watch: Task<()>,
}

struct Shared(Entity<Updates>);

impl Global for Shared {}

/// Starts looking for updates, while the settings ask for it. Vortex calls
/// it once; without it, as in tests, nothing is ever asked of GitHub.
pub fn watch(cx: &mut App) {
    let updates = cx.new(|cx| Updates {
        state: UpdateState::Idle,
        shown_in: None,
        announced: None,
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

    /// Whether the panel shows beside the gear of workspace `id`.
    pub fn shown_in(&self, id: EntityId) -> bool {
        self.shown_in == Some(id)
    }

    pub fn show_in(&mut self, id: Option<EntityId>, cx: &mut Context<Self>) {
        if self.shown_in != id {
            self.shown_in = id;
            cx.notify();
        }
    }

    /// Asks GitHub for the newest release. Asked from workspace `by`, the
    /// panel beside its gear shows how it goes.
    pub fn check(&mut self, by: Option<EntityId>, cx: &mut Context<Self>) {
        if by.is_some() {
            self.show_in(by, cx);
        }
        if !matches!(
            self.state,
            UpdateState::Idle | UpdateState::UpToDate | UpdateState::Failed(_)
        ) {
            return;
        }
        self.state = UpdateState::Checking;
        cx.notify();
        let asked = cx.background_spawn(async { vortex_update::latest() });
        self.checking = cx.spawn(async move |this, cx| {
            let found = asked.await;
            let _ = this.update(cx, |this, cx| this.checked(found, cx));
        });
    }

    fn checked(&mut self, found: anyhow::Result<Release>, cx: &mut Context<Self>) {
        self.state = match found {
            Ok(release) if release.is_newer() => {
                self.announce(release.version, cx);
                UpdateState::Available(release)
            }
            Ok(_) => UpdateState::UpToDate,
            Err(error) => {
                log::warn!("cannot look for a Vortex update: {error:#}");
                UpdateState::Failed(format!("{error:#}"))
            }
        };
        cx.notify();
    }

    /// Opens the panel, once for each release, in the window the user
    /// works in, when no window shows it already.
    fn announce(&mut self, version: Version, cx: &App) {
        if self.announced == Some(version) {
            return;
        }
        self.announced = Some(version);
        if self.shown_in.is_none() {
            let active = cx.try_global::<crate::commands::ActiveWorkspace>();
            self.shown_in = active.map(|active| active.workspace.entity_id());
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

#[cfg(test)]
mod tests {
    use gpui_kit::TestAppContext;

    use super::*;

    fn release(version: &str) -> Release {
        Release {
            version: Version::parse(version).unwrap(),
            notes: String::new(),
            page: String::new(),
            download: None,
        }
    }

    #[gpui_kit::test]
    fn a_release_opens_the_panel_once_and_an_old_one_is_the_newest(cx: &mut TestAppContext) {
        let updates = cx.new(|_| Updates {
            state: UpdateState::Idle,
            shown_in: None,
            announced: None,
            checking: Task::ready(()),
            _watch: Task::ready(()),
        });
        let window = cx.new(|_| ()).entity_id();
        updates.update(cx, |updates, cx| {
            updates.show_in(Some(window), cx);
            updates.checked(Ok(release("99.0.0")), cx);
            assert!(matches!(updates.state(), UpdateState::Available(_)));
            assert!(updates.shown_in(window));
            updates.show_in(None, cx);
            updates.checked(Ok(release("99.0.0")), cx);
            assert!(!updates.shown_in(window), "told once");
            updates.checked(Ok(release("0.0.1")), cx);
            assert_eq!(updates.state(), &UpdateState::UpToDate);
        });
    }
}
