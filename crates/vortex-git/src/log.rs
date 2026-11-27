//! The commits behind the branch, and what each changed.

use std::{
    path::Path,
    time::{Duration, SystemTime},
};

use crate::git;

/// Between fields and after each commit; neither is in a commit's text.
const FIELD: char = '\u{1f}';
const RECORD: char = '\u{1e}';

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit {
    pub id: String,
    pub subject: String,
    pub author: String,
    pub time: SystemTime,
}

impl Commit {
    #[must_use]
    pub fn short_id(&self) -> &str {
        self.id.get(..7).unwrap_or(&self.id)
    }
}

/// The newest `most` commits of the checked-out branch; none in a
/// repository without commits.
///
/// # Errors
/// When `git` cannot run or `root` is not in a repository.
pub fn log(root: &Path, most: usize) -> anyhow::Result<Vec<Commit>> {
    let count = format!("--max-count={most}");
    let format = format!("--format=%H{FIELD}%an{FIELD}%at{FIELD}%s{RECORD}");
    // 128 is also what git exits with before the first commit.
    let output = git(root, &["log", "--no-color", &count, &format], &[0, 128])?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr);
        if message.contains("does not have any commits") {
            return Ok(Vec::new());
        }
        anyhow::bail!("git log failed: {}", message.trim());
    }
    Ok(parse(&String::from_utf8_lossy(&output.stdout)))
}

/// What commit `id` changed, against its first parent, as a patch.
///
/// # Errors
/// When `git` cannot run or there is no such commit.
pub fn show(root: &Path, id: &str) -> anyhow::Result<String> {
    let args = [
        "show",
        "--format=",
        "--no-color",
        "--no-ext-diff",
        "--no-textconv",
        "--find-renames",
        "--diff-merges=first-parent",
        "--src-prefix=a/",
        "--dst-prefix=b/",
        id,
        "--",
    ];
    let output = git(root, &args, &[0])?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn parse(text: &str) -> Vec<Commit> {
    text.split(RECORD)
        .filter_map(|record| {
            let mut fields = record.trim_start_matches('\n').split(FIELD);
            let id = fields.next().filter(|id| !id.is_empty())?.to_owned();
            let author = fields.next()?.to_owned();
            let seconds = fields.next()?.parse().ok()?;
            let subject = fields.next()?.to_owned();
            let time = SystemTime::UNIX_EPOCH + Duration::from_secs(seconds);
            Some(Commit {
                id,
                subject,
                author,
                time,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_are_read_field_by_field() {
        let text = "abcdef123\u{1f}Ada\u{1f}60\u{1f}Fix it\u{1e}\nfff\u{1f}Bo\u{1f}0\u{1f}a\u{1f}b\u{1e}\n";
        let commits = parse(text);
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].short_id(), "abcdef1");
        assert_eq!(
            commits[0].time,
            SystemTime::UNIX_EPOCH + Duration::from_secs(60)
        );
        assert_eq!(commits[1].subject, "a");
    }
}
