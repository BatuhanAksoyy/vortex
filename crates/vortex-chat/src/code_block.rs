//! The bordered, scrollable code box used for tool output and file diffs.
//! Each row's text is selectable on its own and takes part in the window's
//! selection, so a drag across rows copies them without their numbers.
//! Long lines wrap: the box is as wide as the chat and does not scroll
//! sideways.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Pixels, TestSupportExt,
    base::SelectableText,
    component::{ActiveTheme, h_flex, v_flex},
    div, px,
};
use vortex_agent::Hunk;

use crate::transcript::EntryId;

/// Rows kept per block; the rest would never be read in a chat.
const MAX_ROWS: usize = 200;
/// The low bits of a row's document order, which hold its index.
const ROW_BITS: u32 = 16;
/// About eight rows, like the reference panel, before the box scrolls.
const MAX_HEIGHT: f32 = 168.;
const GUTTER_WIDTH: f32 = 36.;
/// About one digit of the monospace font at its usual size.
const DIGIT_WIDTH: f32 = 8.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Plain,
    Added,
    Removed,
}

pub struct CodeLine {
    pub text: String,
    pub kind: LineKind,
    /// The line's number in its file, for a line of a patch.
    pub number: Option<usize>,
}

impl CodeLine {
    pub fn new(text: impl Into<String>, kind: LineKind) -> Self {
        Self {
            text: text.into(),
            kind,
            number: None,
        }
    }

    #[must_use]
    pub fn numbered(mut self, number: usize) -> Self {
        self.number = Some(number);
        self
    }
}

/// A patch's lines as `git diff` shows them: each numbered in the file it
/// stands in (a removed line in the old one), with a gap row between hunks.
pub fn patch_lines(patch: &[Hunk]) -> Vec<CodeLine> {
    let mut lines = Vec::new();
    for (index, hunk) in patch.iter().enumerate() {
        if index > 0 {
            lines.push(CodeLine::new("⋯", LineKind::Plain));
        }
        let (mut old, mut new) = (hunk.old_start, hunk.new_start);
        for line in &hunk.lines {
            let (kind, text) = match line.split_at_checked(1) {
                Some(("-", text)) => (LineKind::Removed, text),
                Some(("+", text)) => (LineKind::Added, text),
                Some((" ", text)) => (LineKind::Plain, text),
                // `\ No newline at end of file` and the like.
                _ => {
                    lines.push(CodeLine::new(line.as_str(), LineKind::Plain));
                    continue;
                }
            };
            let number = if kind == LineKind::Removed { old } else { new };
            old += usize::from(kind != LineKind::Added);
            new += usize::from(kind != LineKind::Removed);
            lines.push(CodeLine::new(text, kind).numbered(number));
        }
    }
    lines
}

/// The box for transcript entry `entry`. `numbered` shows line numbers
/// from 1, which is only true for whole files and command output.
pub fn render(entry: EntryId, lines: &[CodeLine], numbered: bool, cx: &App) -> AnyElement {
    let theme = cx.theme();
    // Plain unnumbered output (a terminal) has nothing to put in a gutter.
    let gutter_shown = numbered
        || lines
            .iter()
            .any(|line| line.kind != LineKind::Plain || line.number.is_some());
    // A patch shows numbers in the gutter, so its signs go before the text.
    let signed = lines.iter().any(|line| line.number.is_some());
    let gutter_width = gutter_width(lines);
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
            let gutter = match (numbered, line.number) {
                (true, _) => (index + 1).to_string(),
                (false, Some(number)) => number.to_string(),
                (false, None) if signed => String::new(),
                (false, None) => marker.to_owned(),
            };
            h_flex()
                .when(!gutter_shown, Styled::pl_2)
                .when(gutter_shown, |this| {
                    this.child(
                        div()
                            .w(gutter_width)
                            .flex_none()
                            .pr_3()
                            .text_right()
                            .text_color(theme.muted_foreground)
                            .child(gutter),
                    )
                })
                .items_start()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .px_1()
                        .when_some(background, Styled::bg)
                        .child(row_text(entry, index, shown_text(line, marker, signed))),
                )
        });
    div()
        .id(("tool-output", entry))
        .test_support()
        .max_h(px(MAX_HEIGHT))
        .overflow_y_scroll()
        .rounded(theme.radius)
        .border_1()
        .border_color(theme.border)
        .bg(theme.secondary)
        .text_sm()
        .font_family(theme.mono_font_family.clone())
        .child(v_flex().py_1().children(rows).children(left_out(lines, cx)))
        .into_any_element()
}

/// A row's text, placed in reading order: the window copies a selection
/// in its participants' document order, which would otherwise be the same
/// for every row and leave them in no particular order.
fn row_text(entry: EntryId, index: usize, text: String) -> SelectableText {
    let row = u64::try_from(index).unwrap_or(u64::MAX);
    SelectableText::new(("line", index), text).document_order(entry << ROW_BITS | row)
}

/// Says how many rows past `MAX_ROWS` the box leaves out, if any.
fn left_out(lines: &[CodeLine], cx: &App) -> Option<gpui_kit::Div> {
    let left_out = lines.len().checked_sub(MAX_ROWS).filter(|n| *n > 0)?;
    Some(
        div()
            .px_2()
            .text_color(cx.theme().muted_foreground)
            .child(format!("{left_out} more lines not shown")),
    )
}

/// Wide enough for the largest line number a patch shows.
fn gutter_width(lines: &[CodeLine]) -> Pixels {
    let largest = lines
        .iter()
        .filter_map(|line| line.number)
        .max()
        .unwrap_or(0);
    let digits = largest.checked_ilog10().map_or(1, |log| log + 1);
    px(GUTTER_WIDTH.max(f32::from(u16::try_from(digits).unwrap_or(u16::MAX)) * DIGIT_WIDTH + 16.))
}

fn shown_text(line: &CodeLine, marker: &str, signed: bool) -> String {
    match (signed, line.text.is_empty()) {
        (true, _) => format!("{:1} {}", marker, line.text),
        (false, true) => " ".to_owned(),
        (false, false) => line.text.clone(),
    }
}

#[cfg(test)]
mod tests {
    use vortex_agent::Hunk;

    use super::{LineKind, patch_lines};

    fn hunk(old_start: usize, new_start: usize, lines: &[&str]) -> Hunk {
        let lines = lines.iter().map(|line| (*line).to_owned()).collect();
        Hunk {
            old_start,
            new_start,
            lines,
        }
    }

    #[test]
    fn a_patch_numbers_each_line_in_its_own_file() {
        let patch = [
            hunk(3, 3, &[" three", "-four", "+FOUR", "+4b", " five"]),
            hunk(20, 21, &["-twenty", "\\ No newline at end of file"]),
        ];
        let shown: Vec<_> = patch_lines(&patch)
            .into_iter()
            .map(|line| (line.number, line.kind, line.text))
            .collect();
        let line = |number, kind, text: &str| (number, kind, text.to_owned());
        assert_eq!(
            shown,
            [
                line(Some(3), LineKind::Plain, "three"),
                line(Some(4), LineKind::Removed, "four"),
                line(Some(4), LineKind::Added, "FOUR"),
                line(Some(5), LineKind::Added, "4b"),
                line(Some(6), LineKind::Plain, "five"),
                line(None, LineKind::Plain, "⋯"),
                line(Some(20), LineKind::Removed, "twenty"),
                line(None, LineKind::Plain, "\\ No newline at end of file"),
            ]
        );
    }
}
