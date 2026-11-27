//! What the window offers of Vortex's updates, all at the gear: a dot on it
//! while there is one to install or restart into, the first item of its
//! menu, and a panel beside it that "Check for Updates…" or a release found
//! on its own opens, with the button that installs it.

use gpui_kit::prelude::*;
use gpui_kit::{
    Anchor, AnyElement, App, Context, Entity, Subscription, WeakEntity, Window,
    component::{
        ActiveTheme as _, Sizable as _, WindowExt as _,
        button::{Button, ButtonVariants as _},
        h_flex,
        menu::PopupMenuItem,
        notification::Notification,
        popover::Popover,
        v_flex,
    },
    div, px,
};

use super::Workspace;
use crate::updates::{self, UpdateState, Updates};

const PANEL_WIDTH: f32 = 280.;

impl Workspace {
    /// Redraws on the updates' changes.
    pub(super) fn follow_updates(_: &mut Window, cx: &mut Context<Self>) -> Vec<Subscription> {
        let Some(updates) = updates::shared(cx) else {
            return Vec::new();
        };
        vec![cx.observe(&updates, |_, _, cx| cx.notify())]
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

    /// Whether an update waits to be installed or restarted into.
    pub(super) fn update_waiting(cx: &App) -> bool {
        updates::shared(cx).is_some_and(|updates| {
            matches!(
                updates.read(cx).state(),
                UpdateState::Available(_) | UpdateState::Ready(..)
            )
        })
    }

    /// What there is to do with updates, else a check: the gear menu's
    /// item and the start page's button.
    pub(super) fn update_label(cx: &App) -> String {
        let state = updates::shared(cx).map(|updates| updates.read(cx).state().clone());
        match state {
            Some(UpdateState::Available(release)) => format!("Update to {}…", release.version),
            Some(UpdateState::Ready(..)) => "Restart to Update".to_owned(),
            _ => "Check for Updates…".to_owned(),
        }
    }

    pub(super) fn update_menu_item(view: &WeakEntity<Self>, cx: &App) -> PopupMenuItem {
        let view = view.clone();
        PopupMenuItem::new(Self::update_label(cx)).on_click(move |_, window, cx| {
            let _ = view.update(cx, |this, cx| this.check_for_updates(window, cx));
        })
    }

    /// The panel at a corner of the element it is put in, which must be
    /// `relative`: the gear's corner toward the panels, opening above it
    /// (`BottomLeft`, `BottomRight`), or the start page's button, opening
    /// below it (`TopLeft`).
    pub(super) fn render_update_panel(anchor: Anchor, cx: &Context<Self>) -> Option<AnyElement> {
        let updates = updates::shared(cx)?;
        let open = updates.read(cx).shown_in(cx.entity_id());
        let closer = updates.clone();
        let popover = Popover::new("update-panel")
            .anchor(anchor)
            .open(open)
            .on_open_change(move |open, _, cx| {
                if !*open {
                    closer.update(cx, |updates, cx| updates.show_in(None, cx));
                }
            })
            // Never clicked: the panel is opened for the gear, not by this.
            .trigger(Button::new("update-panel-anchor").ghost().size(px(1.)))
            .content(move |_, _, cx| panel(&updates, cx));
        let corner = div().absolute().bottom_0().child(popover);
        Some(
            match anchor {
                Anchor::BottomLeft => corner.right_0(),
                _ => corner.left_0(),
            }
            .into_any_element(),
        )
    }
}

fn panel(updates: &Entity<Updates>, cx: &App) -> AnyElement {
    let current = vortex_update::CURRENT;
    let (title, detail, action) = match updates.read(cx).state().clone() {
        UpdateState::Idle | UpdateState::Checking => {
            ("Checking for updates…".to_owned(), None, None)
        }
        UpdateState::UpToDate => (
            format!("Vortex {current} is the newest version."),
            None,
            None,
        ),
        UpdateState::Available(release) => (
            format!("Vortex {} is out.", release.version),
            Some(format!("You have {current}.")),
            Some(Action::Install),
        ),
        UpdateState::Installing(release) => (
            format!("Updating to {}…", release.version),
            None,
            Some(Action::Installing),
        ),
        UpdateState::Ready(release, _) => (
            format!("Vortex {} is ready.", release.version),
            Some("It starts in place of this one.".to_owned()),
            Some(Action::Restart),
        ),
        UpdateState::Failed(error) => (
            "Cannot look for updates.".to_owned(),
            Some(error),
            Some(Action::Retry),
        ),
    };
    let muted = cx.theme().muted_foreground;
    v_flex()
        .id("update-panel-content")
        .w(px(PANEL_WIDTH))
        .gap_2()
        .text_sm()
        .child(div().child(title))
        .children(detail.map(|detail| div().text_xs().text_color(muted).child(detail)))
        .children(action.map(|action| buttons(action, updates)))
        .into_any_element()
}

#[derive(Clone, Copy)]
enum Action {
    Install,
    Installing,
    Restart,
    Retry,
}

fn buttons(action: Action, updates: &Entity<Updates>) -> impl IntoElement + use<> {
    let updates = updates.clone();
    let (label, loading) = match action {
        Action::Install => ("Update Now", false),
        Action::Installing => ("Updating…", true),
        Action::Restart => ("Restart to Update", false),
        Action::Retry => ("Try Again", false),
    };
    h_flex().justify_end().child(
        Button::new("update-panel-action")
            .primary()
            .small()
            .label(label)
            .loading(loading)
            .on_click(move |_, _, cx| {
                updates.update(cx, |updates, cx| match action {
                    Action::Install => updates.install(cx),
                    Action::Restart => updates.restart(cx),
                    Action::Retry => updates.check(None, cx),
                    Action::Installing => {}
                });
            }),
    )
}
