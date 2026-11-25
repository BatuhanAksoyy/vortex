//! The Search page's left side: the box, and what it found.

use std::ops::Range;

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, Context, FontWeight, HighlightStyle, SharedString, StyledText, TestSupportExt,
    base::Selectable as _,
    component::{
        ActiveTheme, Sizable as _,
        button::{Button, ButtonVariants as _},
        h_flex,
        input::Input,
        v_flex,
    },
    div, uniform_list,
};
use kiln_project::MAX_HITS;

use super::{Row, SearchPage};
use crate::file_icon;

impl SearchPage {
    pub(super) fn render_results(&self, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let list = uniform_list(
            "search-results",
            self.rows.len(),
            cx.processor(|this, range: Range<usize>, _, cx| this.render_rows(range, cx)),
        )
        .track_scroll(&self.scroll)
        .flex_1()
        .min_h_0();
        v_flex()
            .id("search-side")
            .test_support()
            .track_focus(&self.focus)
            .size_full()
            .border_r_1()
            .border_color(theme.border)
            .child(
                h_flex()
                    .flex_none()
                    .gap_1()
                    .p_2()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(div().flex_1().child(Input::new(&self.query).small()))
                    .child(
                        Button::new("search-match-case")
                            .ghost()
                            .small()
                            .label("Aa")
                            .tooltip("Match Case")
                            .selected(self.match_case)
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_case(cx))),
                    ),
            )
            .child(self.summary(cx))
            .child(list)
    }

    fn summary(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let hits = self.found.hits.len();
        let files = self.rows.len() - hits;
        let text = match (self.running.is_some(), hits) {
            (true, _) => "Searching…".to_owned(),
            (false, 0) if self.query.read(cx).value().is_empty() => String::new(),
            (false, 0) => "No results.".to_owned(),
            (false, _) if self.found.truncated => {
                format!("The first {MAX_HITS} results, in {files} files.")
            }
            (false, 1) => "1 result.".to_owned(),
            (false, _) => format!("{hits} results in {} {}.", files, plural(files, "file")),
        };
        div()
            .id("search-summary")
            .test_support()
            .aria_label(SharedString::from(text.clone()))
            .flex_none()
            .px_3()
            .py_1()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(text)
    }

    fn render_rows(&self, range: Range<usize>, cx: &Context<Self>) -> Vec<AnyElement> {
        let rows = self.rows.get(range.clone()).unwrap_or_default();
        rows.iter()
            .zip(range)
            .map(|(row, at)| match *row {
                Row::File { first, count } => self.file_row(at, first, count, cx),
                Row::Hit(hit) => self.hit_row(hit, cx),
            })
            .collect()
    }

    fn file_row(&self, at: usize, first: usize, count: usize, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let path = self.found.hits[first].path.to_string_lossy().into_owned();
        let (folder, name) = path.rsplit_once('/').unwrap_or(("", &path));
        h_flex()
            .id(("search-file", at))
            .w_full()
            .h_7()
            .gap_1p5()
            .px_2()
            .text_sm()
            .child(file_icon::file(name, cx))
            .child(
                div()
                    .flex_none()
                    .font_weight(FontWeight::SEMIBOLD)
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
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(count.to_string()),
            )
            .into_any_element()
    }

    fn hit_row(&self, hit: usize, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let found = &self.found.hits[hit];
        let picked = self.picked == Some(hit);
        let hover = theme.list_hover;
        let highlight = HighlightStyle {
            background_color: Some(theme.warning.opacity(0.3)),
            ..HighlightStyle::default()
        };
        let text = StyledText::new(found.text.clone())
            .with_highlights([(found.start..found.end, highlight)]);
        h_flex()
            .id(("search-hit", hit))
            .test_support()
            .w_full()
            .h_7()
            .gap_2()
            .pl_6()
            .pr_2()
            .text_sm()
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .when(picked, |row| row.bg(theme.list_active))
            .on_click(cx.listener(move |this, _, window, cx| this.open(hit, window, cx)))
            .child(
                div()
                    .flex_none()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child((found.line + 1).to_string()),
            )
            .child(div().flex_1().min_w_0().truncate().child(text))
            .into_any_element()
    }
}

fn plural(count: usize, noun: &str) -> String {
    if count == 1 {
        noun.to_owned()
    } else {
        format!("{noun}s")
    }
}
