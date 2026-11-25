//! The Git page's history: the branch's latest commits, under the changes.
//! Picking one shows what it changed.

use std::time::SystemTime;

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, Context, FontWeight, SharedString, TestSupportExt,
    component::{ActiveTheme, h_flex, v_flex},
    div,
};
use kiln_chat::scale::ScaledText as _;
use kiln_git::Commit;

use super::GitPage;
use crate::session_list::ago;

/// Enough to look back with; older ones are for `git log`.
const MAX_COMMITS: usize = 200;

impl GitPage {
    /// Reads the commits again; the page changes only if they did.
    pub(super) fn read_log(&mut self, cx: &mut Context<Self>) {
        let root = self
            .repo
            .read(cx)
            .status()
            .map(|status| status.root.clone());
        let Some(root) = root else {
            return;
        };
        let read = cx.background_spawn(async move { kiln_git::log(&root, MAX_COMMITS) });
        self.log_load = Some(cx.spawn(async move |this, cx| {
            let Ok(commits) = read.await else {
                return;
            };
            let _ = this.update(cx, |this, cx| this.show_commits(commits, cx));
        }));
    }

    fn show_commits(&mut self, commits: Vec<Commit>, cx: &mut Context<Self>) {
        if self.commits != commits {
            self.commits = commits;
            cx.notify();
        }
    }

    pub(super) fn pick_commit(&mut self, id: String, cx: &mut Context<Self>) {
        let root = self
            .repo
            .read(cx)
            .status()
            .map(|status| status.root.clone());
        let Some(root) = root else {
            return;
        };
        self.selected = None;
        self.patch = None;
        self.picked_commit = Some(id.clone());
        let read = cx.background_spawn(async move { kiln_git::show(&root, &id) });
        self.diff_load = Some(cx.spawn(async move |this, cx| {
            let patch = read.await;
            let _ = this.update(cx, |this, cx| this.show_diff(patch, cx));
        }));
        cx.notify();
    }

    pub(super) fn picked(&self) -> Option<&Commit> {
        let id = self.picked_commit.as_ref()?;
        self.commits.iter().find(|commit| commit.id == *id)
    }

    pub(super) fn render_history(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let now = SystemTime::now();
        let rows: Vec<AnyElement> = self
            .commits
            .iter()
            .map(|commit| self.commit_row(commit, now, cx))
            .collect();
        v_flex()
            .id("git-history")
            .test_support()
            .size_full()
            .border_r_1()
            .border_t_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex_none()
                    .px_3()
                    .py_1p5()
                    .border_b_1()
                    .border_color(theme.border)
                    .bg(theme.secondary)
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("History"),
            )
            .child(
                v_flex()
                    .id("git-commits")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .when(rows.is_empty(), |list| {
                        list.child(
                            div()
                                .p_3()
                                .text_sm()
                                .text_color(theme.muted_foreground)
                                .child("No commits yet."),
                        )
                    })
                    .children(rows),
            )
    }

    fn commit_row(&self, commit: &Commit, now: SystemTime, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let picked = self.picked_commit.as_ref() == Some(&commit.id);
        let id = commit.id.clone();
        let about = format!(
            "{} · {} · {}",
            commit.short_id(),
            commit.author,
            ago(commit.time, now)
        );
        let hover = theme.list_hover;
        v_flex()
            .id(SharedString::from(format!("git-commit-{}", commit.id)))
            .test_support()
            .aria_label(SharedString::from(commit.subject.clone()))
            .flex_none()
            .px_3()
            .py_1()
            .border_l_2()
            .border_color(gpui_kit::transparent_black())
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .when(picked, |row| {
                row.bg(theme.list_active).border_color(theme.primary)
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.focus(window, cx);
                this.pick_commit(id.clone(), cx);
            }))
            .child(div().text_sm().truncate().child(commit.subject.clone()))
            .child(
                div()
                    .text_caption()
                    .truncate()
                    .text_color(theme.muted_foreground)
                    .child(about),
            )
            .into_any_element()
    }

    /// Over the diff of a picked commit: what it is.
    pub(super) fn commit_bar(&self, cx: &Context<Self>) -> Option<impl IntoElement + use<>> {
        let commit = self.picked()?;
        let theme = cx.theme();
        let when = ago(commit.time, SystemTime::now());
        Some(
            h_flex()
                .id("git-commit-bar")
                .test_support()
                .flex_none()
                .gap_2()
                .px_3()
                .py_1p5()
                .border_b_1()
                .border_color(theme.border)
                .text_sm()
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(commit.subject.clone()),
                )
                .child(div().flex_1())
                .child(
                    div()
                        .flex_none()
                        .text_caption()
                        .text_color(theme.muted_foreground)
                        .child(format!(
                            "{} · {} · {when}",
                            commit.short_id(),
                            commit.author
                        )),
                ),
        )
    }
}
