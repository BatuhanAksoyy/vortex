//! Staging changes and committing them by hand: a button on each change and
//! section, and the message box over the list. The commit runs the user's
//! own `git`, so their hooks and identity apply.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, Context, Entity, SharedString, Subscription, TestSupportExt, Window,
    assets::IconName as Lucide,
    component::{
        ActiveTheme, Disableable as _,
        button::{Button, ButtonVariants as _},
        input::{InputEvent, Textarea, TextareaState},
        v_flex,
    },
    div,
};
use kiln_chat::scale::{ScaledButton as _, ScaledText as _};
use kiln_git::{Area, Change};

use super::GitPage;
use crate::repo_status::RepoStatus;

const PLACEHOLDER: &str = if cfg!(target_os = "macos") {
    "Message (⌘Enter to commit)"
} else {
    "Message (Ctrl+Enter to commit)"
};
const MESSAGE_ROWS: (usize, usize) = (2, 8);
/// Rows show their stage button while hovered.
pub(super) const ROW_GROUP: &str = "git-change-row";

pub(super) fn new_message(window: &mut Window, cx: &mut Context<GitPage>) -> Entity<TextareaState> {
    cx.new(|cx| {
        TextareaState::new(window, cx)
            .auto_grow(MESSAGE_ROWS.0, MESSAGE_ROWS.1)
            .placeholder(PLACEHOLDER)
    })
}

pub(super) fn follow_message(
    message: &Entity<TextareaState>,
    window: &mut Window,
    cx: &mut Context<GitPage>,
) -> Subscription {
    cx.subscribe_in(message, window, |this, _, event, window, cx| match event {
        InputEvent::PressEnter {
            secondary: true, ..
        } => this.commit(window, cx),
        InputEvent::Change => cx.notify(),
        _ => {}
    })
}

/// What a write did, once it ends.
#[derive(Clone, Copy)]
pub(super) enum Done {
    Staging,
    Committed,
    /// A branch was created, so its name box closes.
    Branched,
}

impl GitPage {
    fn stage(&mut self, changes: Vec<Change>, window: &mut Window, cx: &mut Context<Self>) {
        let stage = changes.iter().any(|change| change.area != Area::Staged);
        self.write(window, cx, Done::Staging, move |root| {
            if stage {
                kiln_git::stage(root, &changes)
            } else {
                kiln_git::unstage(root, &changes)
            }
        });
    }

    pub(super) fn commit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_commit(cx) {
            return;
        }
        let message = self.message.read(cx).value().to_string();
        self.write(window, cx, Done::Committed, move |root| {
            kiln_git::commit(root, &message).map(drop)
        });
    }

    /// Something is staged, a message written, and no write is running.
    fn can_commit(&self, cx: &Context<Self>) -> bool {
        let staged = self.repo.read(cx).status().is_some_and(|status| {
            status
                .changes
                .iter()
                .any(|change| change.area == Area::Staged)
        });
        staged && self.write.is_none() && !self.message.read(cx).value().trim().is_empty()
    }

    /// Runs `act` in the repository's top folder off the main thread, one at
    /// a time, then reads the status again.
    pub(super) fn write(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        done: Done,
        act: impl FnOnce(&std::path::Path) -> anyhow::Result<()> + Send + 'static,
    ) {
        let root = self
            .repo
            .read(cx)
            .status()
            .map(|status| status.root.clone());
        let Some(root) = root.filter(|_| self.write.is_none()) else {
            return;
        };
        let running = cx.background_spawn(async move { act(&root) });
        self.write = Some(cx.spawn_in(window, async move |this, cx| {
            let result = running.await.map_err(|error| format!("{error:#}"));
            let _ = this.update_in(cx, |this, window, cx| this.wrote(result, done, window, cx));
        }));
        cx.notify();
    }

    fn wrote(
        &mut self,
        result: Result<(), String>,
        done: Done,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.write = None;
        let ok = result.is_ok();
        self.write_error = result.err().map(SharedString::from);
        match done {
            Done::Committed if ok => self
                .message
                .update(cx, |message, cx| message.set_value("", window, cx)),
            Done::Branched if ok => self.new_branch = None,
            _ => {}
        }
        self.repo.update(cx, RepoStatus::refresh);
        cx.notify();
    }

    pub(super) fn render_commit_box(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let can_commit = self.can_commit(cx);
        v_flex()
            .id("git-commit-box")
            .test_support()
            .flex_none()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(theme.border)
            .child(Textarea::new(&self.message).aria_label("Commit message"))
            .child(
                Button::new("git-commit")
                    .primary()
                    .control()
                    .w_full()
                    .label("Commit")
                    .loading(self.write.is_some())
                    .disabled(!can_commit)
                    .on_click(cx.listener(|this, _, window, cx| this.commit(window, cx))),
            )
            .children(self.write_error.clone().map(|error| {
                div()
                    .id("git-write-error")
                    .test_support()
                    .aria_label(error.clone())
                    .text_caption()
                    .text_color(theme.danger)
                    .child(error)
            }))
    }

    /// Stages a change, or unstages a staged one; seen while its row is
    /// hovered or picked.
    pub(super) fn stage_button(
        &self,
        id: &str,
        change: &Change,
        selected: bool,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let (icon, tip) = stage_look(change.area, false);
        let picked = vec![change.clone()];
        div()
            .flex_none()
            .when(!selected, |this| {
                this.opacity(0.)
                    .group_hover(ROW_GROUP, |style| style.opacity(1.))
            })
            .child(
                Button::new(SharedString::from(format!("git-stage-{id}")))
                    .ghost()
                    .row_icon(icon)
                    .tooltip(tip)
                    .disabled(self.write.is_some())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.stage(picked.clone(), window, cx);
                    })),
            )
    }

    /// Stages, or unstages, every change in `area`.
    pub(super) fn stage_all_button(&self, id: &str, area: Area, cx: &Context<Self>) -> AnyElement {
        let (icon, tip) = stage_look(area, true);
        let changes: Vec<Change> = self.repo.read(cx).status().map_or_else(Vec::new, |status| {
            let in_area = status.changes.iter().filter(|change| change.area == area);
            in_area.cloned().collect()
        });
        Button::new(SharedString::from(format!("git-stage-all-{id}")))
            .ghost()
            .row_icon(icon)
            .tooltip(tip)
            .disabled(self.write.is_some())
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.stage(changes.clone(), window, cx);
            }))
            .into_any_element()
    }
}

fn stage_look(area: Area, all: bool) -> (Lucide, &'static str) {
    match (area, all) {
        (Area::Staged, false) => (Lucide::Minus, "Unstage"),
        (Area::Staged, true) => (Lucide::Minus, "Unstage All"),
        (Area::Conflicted, false) => (Lucide::Check, "Mark as Resolved"),
        (Area::Conflicted, true) => (Lucide::Check, "Mark All as Resolved"),
        (_, false) => (Lucide::Plus, "Stage"),
        (_, true) => (Lucide::Plus, "Stage All"),
    }
}
