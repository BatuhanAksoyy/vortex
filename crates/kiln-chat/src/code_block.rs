//! The bordered, scrollable code box used for tool output and file diffs.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, ElementId, TestSupportExt,
    component::{ActiveTheme, h_flex, v_flex},
    div, px,
};

/// Rows kept per block; the rest would never be read in a chat.
const MAX_ROWS: usize = 400;
/// About eight rows, like the reference panel, before the box scrolls.
const MAX_HEIGHT: f32 = 168.;
const GUTTER_WIDTH: f32 = 36.;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Plain,
    Added,
    Removed,
}

pub struct CodeLine {
    pub text: String,
    pub kind: LineKind,
}

impl CodeLine {
    pub fn new(text: impl Into<String>, kind: LineKind) -> Self {
        Self {
            text: text.into(),
            kind,
        }
    }
}

/// `numbered` shows line numbers from 1, which is only true for whole files
/// and command output.
pub fn render(
    id: impl Into<ElementId>,
    lines: &[CodeLine],
    numbered: bool,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    // Plain unnumbered output (a terminal) has nothing to put in a gutter.
    let gutter_shown = numbered || lines.iter().any(|line| line.kind != LineKind::Plain);
    let rows = lines
        .iter()
        .take(MAX_ROWS)
        .enumerate()
        .map(|(index, line)| {
            let (background, marker) = match line.kind {
                LineKind::Plain => (None, ""),
                LineKind::Added => (Some(theme.success.opacity(0.25)), "+"),
                LineKind::Removed => (Some(theme.danger.opacity(0.25)), "-"),
            };
            let gutter = if numbered {
                (index + 1).to_string()
            } else {
                marker.to_owned()
            };
            h_flex()
                .when(!gutter_shown, Styled::pl_2)
                .when(gutter_shown, |this| {
                    this.child(
                        div()
                            .w(px(GUTTER_WIDTH))
                            .flex_none()
                            .pr_3()
                            .text_right()
                            .text_color(theme.muted_foreground)
                            .child(gutter),
                    )
                })
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .px_1()
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .when_some(background, Styled::bg)
                        .child(if line.text.is_empty() {
                            " ".to_owned()
                        } else {
                            line.text.clone()
                        }),
                )
        });
    div()
        .id(id)
        .test_support()
        .max_h(px(MAX_HEIGHT))
        .overflow_y_scroll()
        .rounded(theme.radius)
        .border_1()
        .border_color(theme.border)
        .bg(theme.secondary)
        .text_sm()
        .font_family(theme.mono_font_family.clone())
        .child(v_flex().py_1().children(rows))
        .into_any_element()
}
