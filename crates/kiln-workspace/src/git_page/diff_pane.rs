//! The Git page's right side: the picked change's path and controls over
//! its diff, or why there is no diff.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, Context, FontWeight, SharedString, TestSupportExt,
    assets::IconName as Lucide,
    base::Selectable as _,
    component::{
        ActiveTheme, Disableable as _, Sizable as _,
        button::{Button, ButtonVariants as _},
        diff::{Diff, DiffFile, DiffMode},
        h_flex, v_flex,
    },
    div,
};
use kiln_git::{Area, Change};

use super::{GitPage, changes::line_counts};
use crate::repo_status::kind_color;

impl GitPage {
    pub(super) fn render_diff(&self, cx: &Context<Self>) -> impl IntoElement {
        let body = match self.note(cx) {
            Some(note) => note_view(note, cx),
            None if self.patch.is_none() => div().into_any_element(),
            None => div()
                .id("git-diff")
                .test_support()
                .size_full()
                .child(
                    Diff::new(&self.diff)
                        .header_visible(self.picked_commit.is_some())
                        .size_full(),
                )
                .into_any_element(),
        };
        v_flex()
            .size_full()
            .children(
                self.selected
                    .as_ref()
                    .map(|change| self.toolbar(change, cx)),
            )
            .children(self.commit_bar(cx))
            .child(div().flex_1().min_h_0().child(body))
    }

    /// What shows in place of a diff.
    fn note(&self, cx: &Context<Self>) -> Option<String> {
        match self.repo.read(cx).reading() {
            None => return Some("Reading the repository…".to_owned()),
            Some(Err(error)) if error.contains("not a git repository") => {
                return Some("This folder is not in a git repository.".to_owned());
            }
            Some(Err(error)) => return Some(format!("Cannot read the repository: {error}")),
            Some(Ok(_)) if self.selected.is_none() && self.picked_commit.is_none() => {
                return Some("Nothing has changed since the last commit.".to_owned());
            }
            Some(Ok(_)) => {}
        }
        if let Err(error) = self.patch.as_ref()? {
            return Some(error.clone());
        }
        let files = self.diff.read(cx).files();
        if files.iter().any(DiffFile::is_binary) {
            return Some("A binary file, which has no text diff.".to_owned());
        }
        let unchanged = !files.iter().any(DiffFile::has_changes);
        unchanged.then(|| "No lines changed.".to_owned())
    }

    fn toolbar(&self, change: &Change, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let counts = self.repo.read(cx).status().and_then(|s| s.counts(change));
        let path = change.path.to_string_lossy().into_owned();
        let state = match change.area {
            Area::Staged => format!("{}, staged", change.kind.label()),
            _ => change.kind.label().to_owned(),
        };
        h_flex()
            .id("git-diff-toolbar")
            .test_support()
            .aria_label(SharedString::from(format!("{path}: {state}")))
            .flex_none()
            .flex_wrap()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex_none()
                    .max_w_full()
                    .truncate()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(path),
            )
            .child(
                div()
                    .flex_none()
                    .text_xs()
                    .text_color(kind_color(change.kind, theme))
                    .child(state),
            )
            .children(counts.map(|counts| line_counts(counts, theme)))
            .child(div().flex_1())
            .child(self.mode_buttons(cx))
            .child(self.step_buttons(cx))
            .child(
                Button::new("git-open-file")
                    .ghost()
                    .small()
                    .icon(Lucide::ExternalLink)
                    .tooltip("Open in Files (Enter)")
                    .disabled(self.openable(cx).is_none())
                    .on_click(cx.listener(|this, _, _, cx| this.open_selected(cx))),
            )
    }

    fn mode_buttons(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let shown = self.diff.read(cx).mode();
        let modes = [
            ("git-unified", "Unified", DiffMode::Unified),
            ("git-split", "Split", DiffMode::Split),
        ];
        h_flex()
            .flex_none()
            .children(modes.map(|(id, label, mode)| {
                let diff = self.diff.clone();
                Button::new(id)
                    .ghost()
                    .small()
                    .label(label)
                    .selected(shown == mode)
                    .on_click(cx.listener(move |_, _, _, cx| {
                        diff.update(cx, |diff, cx| diff.set_mode(mode, cx));
                        cx.notify();
                    }))
            }))
    }

    /// `2 of 7`, with the way to the changes before and after.
    fn step_buttons(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let changes = self.reachable(cx);
        let at = self.position(&changes);
        let place = at.map(|at| format!("{} of {}", at + 1, changes.len()));
        let step = |id, icon, tooltip, forward| {
            Button::new(id)
                .ghost()
                .small()
                .icon(icon)
                .tooltip(tooltip)
                .on_click(cx.listener(move |this, _, _, cx| this.step(forward, cx)))
        };
        let first = at == Some(0) || changes.is_empty();
        let last = at.is_some_and(|at| at + 1 == changes.len()) || changes.is_empty();
        h_flex()
            .flex_none()
            .gap_1()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .children(place)
            .child(
                step(
                    "git-previous",
                    Lucide::ChevronUp,
                    "Previous change (↑)",
                    false,
                )
                .disabled(first),
            )
            .child(step("git-next", Lucide::ChevronDown, "Next change (↓)", true).disabled(last))
    }
}

fn note_view(note: String, cx: &Context<GitPage>) -> AnyElement {
    v_flex()
        .id("git-note")
        .test_support()
        .aria_label(SharedString::from(note.clone()))
        .size_full()
        .p_4()
        .items_center()
        .justify_center()
        .text_center()
        .text_color(cx.theme().muted_foreground)
        .child(note)
        .into_any_element()
}
