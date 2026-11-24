//! Marks in the gutter and on the scrollbar where a file differs from the
//! last commit.

use std::ops::Range;

use gpui_kit::{
    App, Entity, Hsla,
    component::{
        ActiveTheme as _, Theme,
        input::{
            EditorState, GutterDecoration, GutterDecorationCollection, OverviewRulerDecoration,
            OverviewRulerDecorationCollection, OverviewRulerLane,
        },
    },
};
use kiln_git::{LineChange, LineKind};

pub struct Marks {
    gutter: GutterDecorationCollection,
    ruler: OverviewRulerDecorationCollection,
}

impl Marks {
    pub fn new(editor: &Entity<EditorState>, cx: &mut App) -> Self {
        editor.update(cx, |editor, cx| Self {
            gutter: editor.create_gutter_decorations_collection(Vec::new(), cx),
            ruler: editor.create_overview_ruler_decorations_collection(Vec::new(), cx),
        })
    }

    /// Marks `changes`, which are between the last commit and `text`.
    pub fn set(&self, changes: &[LineChange], text: &str, cx: &mut App) {
        let starts = line_starts(text);
        let theme = cx.theme();
        let marks: Vec<(Range<usize>, Hsla)> = changes
            .iter()
            .map(|change| {
                let color = line_color(change.kind, theme);
                (bytes(&change.lines, &starts, text.len()), color)
            })
            .collect();
        let gutter = marks
            .iter()
            .map(|(range, color)| GutterDecoration::new(range.clone()).with_color(*color));
        self.gutter.set(gutter.collect(), cx);
        let ruler = marks.into_iter().map(|(range, color)| {
            OverviewRulerDecoration::new(range)
                .with_lane(OverviewRulerLane::Full)
                .with_color(color)
        });
        self.ruler.set(ruler.collect(), cx);
    }
}

pub fn line_color(kind: LineKind, theme: &Theme) -> Hsla {
    match kind {
        LineKind::Added => theme.success,
        LineKind::Modified => theme.warning,
        LineKind::Deleted => theme.danger,
    }
}

fn line_starts(text: &str) -> Vec<usize> {
    let after_breaks = text.match_indices('\n').map(|(at, _)| at + 1);
    std::iter::once(0).chain(after_breaks).collect()
}

/// The bytes of `lines`, without the last line's ending: with it the range
/// would touch the line below.
fn bytes(lines: &Range<usize>, starts: &[usize], end: usize) -> Range<usize> {
    let start_of = |line: usize| starts.get(line).copied().unwrap_or(end);
    let start = start_of(lines.start);
    if lines.is_empty() {
        return start..start;
    }
    let last = match starts.get(lines.end) {
        Some(next) => next.saturating_sub(1),
        None => end,
    };
    start..last.max(start)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_become_byte_ranges() {
        let text = "ab\n\ncd\n";
        let starts = line_starts(text);
        let range = |lines| bytes(&lines, &starts, text.len());
        assert_eq!(range(0..1), 0..2);
        assert_eq!(range(0..3), 0..6);
        assert_eq!(range(2..2), 4..4);
        assert_eq!(range(3..3), 7..7);
        assert_eq!(range(9..9), 7..7);
    }
}
