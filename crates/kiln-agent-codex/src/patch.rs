//! A file change's unified diff, as Codex reports it, split into hunks.

use kiln_agent::Hunk;

/// The hunks of `diff`; lines before the first `@@` header, such as
/// `--- a/file`, are left out.
pub fn hunks(diff: &str) -> Vec<Hunk> {
    let mut hunks: Vec<Hunk> = Vec::new();
    for line in diff.lines() {
        if let Some((old_start, new_start)) = header(line) {
            hunks.push(Hunk {
                old_start,
                new_start,
                lines: Vec::new(),
            });
        } else if let Some(hunk) = hunks.last_mut() {
            hunk.lines.push(line.to_owned());
        }
    }
    hunks
}

/// The first line numbers an `@@ -3,4 +3,5 @@` header gives.
fn header(line: &str) -> Option<(usize, usize)> {
    let ranges = line.strip_prefix("@@ -")?.split(" @@").next()?;
    let (old, new) = ranges.split_once(" +")?;
    let start = |range: &str| range.split(',').next()?.parse().ok();
    Some((start(old)?, start(new)?))
}

#[cfg(test)]
mod tests {
    use kiln_agent::Hunk;

    use super::hunks;

    #[test]
    fn splits_a_diff_into_hunks() {
        let diff = "--- a/note.txt\n+++ b/note.txt\n@@ -1,3 +1,3 @@\n one\n-two\n+TWO\n three\n@@ -20 +20,2 @@ fn x\n+new\n";
        let lines = |all: &[&str]| all.iter().map(|line| (*line).to_owned()).collect();
        assert_eq!(
            hunks(diff),
            [
                Hunk {
                    old_start: 1,
                    new_start: 1,
                    lines: lines(&[" one", "-two", "+TWO", " three"])
                },
                Hunk {
                    old_start: 20,
                    new_start: 20,
                    lines: lines(&["+new"])
                },
            ]
        );
    }
}
