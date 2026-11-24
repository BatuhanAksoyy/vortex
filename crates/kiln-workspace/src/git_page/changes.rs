//! The Git page's left side: the branch, how much changed, and the changes
//! in one section per area.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Context, FontWeight, SharedString, TestSupportExt,
    assets::IconName as Lucide,
    component::{
        ActiveTheme, Icon, Sizable as _, Theme,
        button::{Button, ButtonVariants as _},
        h_flex,
        tooltip::Tooltip,
        v_flex,
    },
    div,
};
use kiln_git::{Area, Change, Counts, Status};

use super::GitPage;
use crate::{
    file_icon,
    repo_status::{RepoStatus, kind_color},
};

struct Section {
    area: Area,
    id: &'static str,
    title: &'static str,
}

const SECTIONS: [Section; 4] = [
    section(Area::Conflicted, "conflicted", "Merge Conflicts"),
    section(Area::Staged, "staged", "Staged"),
    section(Area::Unstaged, "unstaged", "Not Staged"),
    section(Area::Untracked, "untracked", "Untracked"),
];

const fn section(area: Area, id: &'static str, title: &'static str) -> Section {
    Section { area, id, title }
}

/// A section lists no more rows than this: an unignored build folder would
/// otherwise lay out thousands.
const MAX_ROWS: usize = 300;

impl GitPage {
    pub(super) fn render_changes(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let status = self.repo.read(cx).status();
        let rows = status.map_or_else(Vec::new, |status| self.rows(status, cx));
        v_flex()
            .id("git-changes")
            .test_support()
            .size_full()
            .border_r_1()
            .border_color(theme.border)
            .child(self.summary(status, cx))
            .child(
                v_flex()
                    .id("git-change-list")
                    .test_support()
                    .track_focus(&self.list_focus)
                    .track_scroll(&self.list_scroll)
                    .on_key_down(cx.listener(Self::on_list_key))
                    .flex_1()
                    .min_h_0()
                    .pb_2()
                    .overflow_y_scroll()
                    .text_sm()
                    .children(rows),
            )
    }

    fn summary(&self, status: Option<&Status>, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let branch = match status {
            Some(status) => status.branch.as_deref().unwrap_or("No branch"),
            None => "Git",
        };
        let repo = self.repo.clone();
        v_flex()
            .flex_none()
            .gap_1()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(theme.border)
            .child(
                h_flex()
                    .gap_2()
                    .child(Icon::new(Lucide::GitBranch).small())
                    .child(
                        div()
                            .id("git-branch")
                            .test_support()
                            .aria_label(SharedString::from(branch.to_owned()))
                            .min_w_0()
                            .truncate()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(branch.to_owned()),
                    )
                    .children(status.and_then(|status| distance(status, theme)))
                    .child(div().flex_1())
                    .child(
                        Button::new("git-refresh")
                            .ghost()
                            .small()
                            .icon(Lucide::RefreshCw)
                            .tooltip("Refresh")
                            .on_click(move |_, _, cx| repo.update(cx, RepoStatus::refresh)),
                    ),
            )
            .children(status.map(|status| totals(status, theme)))
    }

    /// The section headings and, under the open ones, their changes, as
    /// direct children of the list so each can be scrolled to.
    fn rows(&self, status: &Status, cx: &Context<Self>) -> Vec<AnyElement> {
        let mut rows = Vec::new();
        for section in SECTIONS {
            let Section { area, id, .. } = section;
            let in_area = |change: &&Change| change.area == area;
            let count = status.changes.iter().filter(in_area).count();
            if count == 0 {
                continue;
            }
            rows.push(self.heading(&section, count, cx));
            if self.collapsed.contains(&area) {
                continue;
            }
            let shown = status.changes.iter().filter(in_area).take(MAX_ROWS);
            rows.extend(shown.map(|change| self.row(id, change, status.counts(change), cx)));
            if count > MAX_ROWS {
                rows.push(more(count - MAX_ROWS, cx));
            }
        }
        rows
    }

    /// Scrolls to `change`'s row, counting the headings above it.
    pub(super) fn scroll_to(&self, change: &Change, cx: &Context<Self>) {
        let changes = self.ordered(cx);
        let mut row = 0;
        for Section { area, .. } in SECTIONS {
            let section: Vec<&Change> = changes.iter().filter(|c| c.area == area).collect();
            if section.is_empty() {
                continue;
            }
            row += 1;
            if area == change.area {
                row += section.iter().position(|c| *c == change).unwrap_or(0);
                break;
            }
            if !self.collapsed.contains(&area) {
                row += section.len().min(MAX_ROWS) + usize::from(section.len() > MAX_ROWS);
            }
        }
        self.list_scroll.scroll_to_item(row);
    }

    fn heading(&self, section: &Section, count: usize, cx: &Context<Self>) -> AnyElement {
        let &Section { area, id, title } = section;
        let theme = cx.theme();
        let chevron = if self.collapsed.contains(&area) {
            Lucide::ChevronRight
        } else {
            Lucide::ChevronDown
        };
        h_flex()
            .id(SharedString::from(format!("git-section-{id}")))
            .test_support()
            .aria_label(SharedString::from(format!("{title}, {count}")))
            .flex_none()
            .gap_1p5()
            .px_2()
            .py_1p5()
            .border_b_1()
            .border_color(theme.border)
            .bg(theme.secondary)
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| this.toggle_section(area, cx)))
            .child(
                Icon::new(chevron)
                    .xsmall()
                    .text_color(theme.muted_foreground),
            )
            .child(
                div()
                    .flex_1()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(count.to_string()),
            )
            .into_any_element()
    }

    fn row(
        &self,
        section: &str,
        change: &Change,
        counts: Option<Counts>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let path = change.path.to_string_lossy().into_owned();
        let (folder, name) = path.rsplit_once('/').unwrap_or(("", &path));
        let selected = self.selected.as_ref() == Some(change);
        let color = kind_color(change.kind, theme);
        let hover = theme.list_hover;
        let picked = change.clone();
        let tip = tip(change, &path);
        h_flex()
            .id(SharedString::from(format!("git-change-{section}-{path}")))
            .test_support()
            .aria_label(SharedString::from(path.clone()))
            .flex_none()
            .gap_2()
            .pl_2()
            .pr_3()
            .py_1()
            .border_l_2()
            .border_color(gpui_kit::transparent_black())
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .when(selected, |this| {
                this.bg(theme.list_active).border_color(theme.primary)
            })
            .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.focus(window, cx);
                this.select(Some(picked.clone()), cx);
            }))
            .child(
                div()
                    .flex_none()
                    .w_5()
                    .text_center()
                    .text_xs()
                    .font_weight(FontWeight::BOLD)
                    .rounded(theme.radius)
                    .bg(color.opacity(0.15))
                    .text_color(color)
                    .child(change.kind.letter().to_string()),
            )
            .child(file_icon::file(name, cx))
            .child(
                div()
                    .flex_none()
                    .max_w_2_3()
                    .truncate()
                    .child(name.to_owned()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(folder.to_owned()),
            )
            .children(counts.map(|counts| line_counts(counts, theme)))
            .into_any_element()
    }
}

/// What the row's letter means, and the whole path.
fn tip(change: &Change, path: &str) -> SharedString {
    let kind = change.kind.label();
    match &change.original {
        Some(original) => format!("{kind}: {} → {path}", original.display()),
        None => format!("{kind}: {path}"),
    }
    .into()
}

fn more(hidden: usize, cx: &App) -> AnyElement {
    div()
        .flex_none()
        .px_3()
        .py_1()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(format!("{hidden} more not shown"))
        .into_any_element()
}

/// `↑2 ↓1`: how far the branch is from its upstream.
fn distance(status: &Status, theme: &Theme) -> Option<impl IntoElement + use<>> {
    let parts: Vec<String> = [("↑", status.ahead), ("↓", status.behind)]
        .into_iter()
        .filter(|(_, commits)| *commits > 0)
        .map(|(arrow, commits)| format!("{arrow}{commits}"))
        .collect();
    (!parts.is_empty()).then(|| {
        div()
            .id("git-distance")
            .flex_none()
            .text_xs()
            .text_color(theme.muted_foreground)
            .tooltip(|window, cx| {
                Tooltip::new("Commits to push (↑) and to pull (↓)").build(window, cx)
            })
            .child(parts.join(" "))
    })
}

fn totals(status: &Status, theme: &Theme) -> impl IntoElement + use<> {
    let files: std::collections::HashSet<_> =
        status.changes.iter().map(|change| &change.path).collect();
    let text = match files.len() {
        0 => "No changes".to_owned(),
        1 => "1 file changed".to_owned(),
        count => format!("{count} files changed"),
    };
    let mut total = Counts::default();
    for counts in status.changes.iter().filter_map(|c| status.counts(c)) {
        total.added += counts.added;
        total.removed += counts.removed;
    }
    h_flex()
        .gap_2()
        .text_xs()
        .text_color(theme.muted_foreground)
        .child(text)
        .child(line_counts(total, theme))
}

/// `+12 −3`, leaving out a side with none.
pub(super) fn line_counts(counts: Counts, theme: &Theme) -> impl IntoElement + use<> {
    let side = |sign: &str, lines: usize, color| {
        (lines > 0).then(|| div().text_color(color).child(format!("{sign}{lines}")))
    };
    h_flex()
        .flex_none()
        .gap_1()
        .text_xs()
        .children(side("+", counts.added, theme.success))
        .children(side("−", counts.removed, theme.danger))
}
