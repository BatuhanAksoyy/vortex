//! What the window offers of Vortex's updates: a button in the status bar
//! while there is one to install or restart into, and the answer to
//! "Check for Updates…".

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, Context, Subscription, Window,
    component::{
        WindowExt as _,
        button::{Button, ButtonVariants as _},
        notification::Notification,
    },
};
use vortex_chat::scale::ScaledButton as _;

use super::Workspace;
use crate::updates::{self, UpdateState, UpdatesEvent};

impl Workspace {
    /// Redraws on the updates' changes, and tells this window what a check
    /// asked here found.
    pub(super) fn follow_updates(window: &mut Window, cx: &mut Context<Self>) -> Vec<Subscription> {
        let Some(updates) = updates::shared(cx) else {
            return Vec::new();
        };
        vec![
            cx.observe(&updates, |_, _, cx| cx.notify()),
            cx.subscribe_in(&updates, window, |_, _, event, window, cx| {
                let UpdatesEvent::Checked { by, found } = event;
                if *by == cx.entity_id() {
                    Self::tell_checked(found, window, cx);
                }
            }),
        ]
    }

    pub fn check_for_updates(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(updates) = updates::shared(cx) else {
            let note = Notification::info("This build of Vortex does not look for updates.");
            window.push_notification(note, cx);
            return;
        };
        let by = cx.entity_id();
        updates.update(cx, |updates, cx| updates.check(Some(by), cx));
    }

    fn tell_checked(
        found: &Result<Option<vortex_update::Release>, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let note = match found {
            Ok(Some(release)) => Notification::info(format!(
                "Vortex {} is out; the status bar offers to update.",
                release.version
            )),
            Ok(None) => Notification::info(format!(
                "Vortex {} is the newest version.",
                vortex_update::CURRENT
            )),
            Err(error) => Notification::error(format!("Cannot look for updates: {error}")),
        };
        window.push_notification(note, cx);
    }

    pub(super) fn render_update(cx: &Context<Self>) -> Option<AnyElement> {
        let updates = updates::shared(cx)?;
        let (label, tooltip) = match updates.read(cx).state() {
            UpdateState::Available(release) => (
                format!("Update to {}", release.version),
                "Download and Install the Update",
            ),
            UpdateState::Installing(release) => (format!("Updating to {}…", release.version), ""),
            UpdateState::Ready(..) => ("Restart to Update".to_owned(), "Start the Updated Vortex"),
            UpdateState::Idle | UpdateState::Checking | UpdateState::Failed(_) => return None,
        };
        let installing = matches!(updates.read(cx).state(), UpdateState::Installing(_));
        let button = Button::new("status-update")
            .primary()
            .row_labelled(gpui_kit::assets::IconName::CircleArrowUp, label)
            .loading(installing)
            .when(!tooltip.is_empty(), |button| button.tooltip(tooltip))
            .on_click(move |_, _, cx| {
                updates.update(cx, |updates, cx| match updates.state() {
                    UpdateState::Available(_) => updates.install(cx),
                    UpdateState::Ready(..) => updates.restart(cx),
                    _ => {}
                });
            });
        Some(button.into_any_element())
    }
}
