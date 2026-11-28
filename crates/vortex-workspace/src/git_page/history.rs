//! The Git page's history: the latest commits of the branches, under the
//! changes, with the graph of how they join. Picking one shows what it
//! changed.

use std::time::SystemTime;

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, Context, FontWeight, Pixels, SharedString, TestSupportExt,
    component::{ActiveTheme, h_flex, v_flex},
    div, px,
};
use vortex_chat::scale::ScaledText as _;
use vortex_git::{Commit, GraphRow};

use super::{
    GitPage,
    graph_lanes::{self, Palette},
};
use crate::session_list::ago;

/// Enough to look back with; older ones are for `git log`.
const MAX_COMMITS: usize = 200;
/// Past it a branch's name at its commit is cut short.
const REF_MAX_WIDTH: f32 = 96.;

/// What every commit's row is drawn with.
#[derive(Clone, Copy)]
struct Shared {
    /// The graph's, beside each row.
    width: Pixels,
    palette: Palette,
    now: SystemTime,
}

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
        let read = cx.background_spawn(async move { vortex_git::log(&root, MAX_COMMITS) });
        self.log_load = Some(cx.spawn(async move |this, cx| {
            let Ok(commits) = read.await else {
                return;
            };
            let _ = this.update(cx, |this, cx| this.show_commits(commits, cx));
        }));
    }

    fn show_commits(&mut self, commits: Vec<Commit>, cx: &mut Context<Self>) {
        if self.commits != commits {
            self.graph = vortex_git::graph(&commits);
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
        let read = cx.background_spawn(async move { vortex_git::show(&root, &id) });
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
        let shared = Shared {
            width: graph_lanes::width(&self.graph),
            palette: Palette::of(cx),
            now: SystemTime::now(),
        };
        let rows: Vec<AnyElement> = (self.commits.iter().zip(&self.graph))
            .map(|(commit, row)| self.commit_row(commit, row, shared, cx))
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

    fn commit_row(
        &self,
        commit: &Commit,
        row: &GraphRow,
        shared: Shared,
        cx: &Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let picked = self.picked_commit.as_ref() == Some(&commit.id);
        let id = commit.id.clone();
        let hover = theme.list_hover;
        // Not an `h_flex`, which centers: the graph stretches to the row's height, so
        // its lines meet those of the rows above and below.
        div()
            .id(SharedString::from(format!("git-commit-{}", commit.id)))
            .test_support()
            .aria_label(SharedString::from(commit.subject.clone()))
            .flex()
            .flex_none()
            .gap_1()
            .pl_1()
            .pr_3()
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
            .child(graph_lanes::render(
                row.clone(),
                shared.width,
                shared.palette,
            ))
            .child(commit_text(commit, shared.now, cx))
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

/// The commit's branches and subject over who made it and when.
fn commit_text(
    commit: &Commit,
    now: SystemTime,
    cx: &Context<GitPage>,
) -> impl IntoElement + use<> {
    let about = format!(
        "{} · {} · {}",
        commit.short_id(),
        commit.author,
        ago(commit.time, now)
    );
    v_flex()
        .flex_1()
        .min_w_0()
        .py_1()
        .child(
            h_flex()
                .gap_1()
                .min_w_0()
                .children(commit.refs.iter().map(|name| ref_badge(name, cx)))
                .child(
                    div()
                        .min_w_0()
                        .text_sm()
                        .truncate()
                        .child(commit.subject.clone()),
                ),
        )
        .child(
            div()
                .text_caption()
                .truncate()
                .text_color(cx.theme().muted_foreground)
                .child(about),
        )
}

/// A branch or tag at a commit, before its subject.
fn ref_badge(name: &str, cx: &Context<GitPage>) -> impl IntoElement + use<> {
    let theme = cx.theme();
    div()
        .flex_none()
        .max_w(px(REF_MAX_WIDTH))
        .px_1()
        .rounded(theme.radius)
        .border_1()
        .border_color(theme.border)
        .text_caption()
        .text_color(theme.muted_foreground)
        .truncate()
        .child(name.to_owned())
}
