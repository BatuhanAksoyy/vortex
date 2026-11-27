//! How a file's text differs from the last commit, line by line.

use std::{
    ops::Range,
    path::Path,
    time::{Duration, Instant},
};

use similar::{Algorithm, DiffOp, TextDiff, capture_diff_slices_deadline};

use crate::git;

/// Past this the diff gets coarser rather than later.
const TIME_LIMIT: Duration = Duration::from_millis(500);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Added,
    Modified,
    Deleted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineChange {
    pub kind: LineKind,
    /// Lines of the new text, from 0. Empty for a deletion, at the line the
    /// deleted ones were above.
    pub lines: Range<usize>,
}

/// The text of the file at `path` in the last commit; `None` when it is not
/// in it.
#[must_use]
pub fn base_text(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    let output = git(path.parent()?, &["show", &format!("HEAD:./{name}")], &[0]).ok()?;
    String::from_utf8(output.stdout).ok()
}

/// What changed from `base` to `text`, in order.
#[must_use]
pub fn line_changes(base: &str, text: &str) -> Vec<LineChange> {
    // Without their endings, so a checkout with CRLF is not all changed.
    let old: Vec<&str> = base.lines().collect();
    let new: Vec<&str> = text.lines().collect();
    let deadline = Some(Instant::now() + TIME_LIMIT);
    capture_diff_slices_deadline(Algorithm::Myers, &old, &new, deadline)
        .iter()
        .filter_map(|op| {
            let kind = match op {
                DiffOp::Equal { .. } => return None,
                DiffOp::Delete { .. } => LineKind::Deleted,
                DiffOp::Insert { .. } => LineKind::Added,
                DiffOp::Replace { .. } => LineKind::Modified,
            };
            let lines = op.new_range();
            Some(LineChange { kind, lines })
        })
        .collect()
}

/// The patch from `base` to `text` of the file called `path`; empty when
/// they are the same.
#[must_use]
pub fn patch(path: &str, base: &str, text: &str) -> String {
    let diff = TextDiff::configure()
        .timeout(TIME_LIMIT)
        .diff_lines(base, text);
    diff.unified_diff()
        .header(&format!("a/{path}"), &format!("b/{path}"))
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(kind: LineKind, lines: Range<usize>) -> LineChange {
        LineChange { kind, lines }
    }

    #[test]
    fn lines_are_added_modified_and_deleted() {
        let changes = line_changes("a\nb\nc\nd\ne\n", "a\nB\nc\ne\nf\n");
        assert_eq!(
            changes,
            [
                change(LineKind::Modified, 1..2),
                change(LineKind::Deleted, 3..3),
                change(LineKind::Added, 4..5),
            ]
        );
        assert_eq!(line_changes("a\r\nb\r\n", "a\nb\n"), []);
    }

    #[test]
    fn a_patch_names_the_file() {
        let patch = patch("src/a.rs", "a\n", "b\n");
        assert!(
            patch.contains("--- a/src/a.rs\n+++ b/src/a.rs\n"),
            "{patch}"
        );
        assert!(patch.contains("-a\n+b\n"), "{patch}");
        assert_eq!(super::patch("a", "same\n", "same\n"), "");
    }
}
