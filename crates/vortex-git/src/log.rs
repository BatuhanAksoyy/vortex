//! The commits behind the branches, and what each changed.

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
    /// The first is the one the commit was made on; a merge has more.
    pub parents: Vec<String>,
    /// The branches and tags pointing at it.
    pub refs: Vec<String>,
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

/// The newest `most` commits of the checked-out branch and the other local
/// ones, each after every commit made on it; none in a repository without
/// commits.
///
/// # Errors
/// When `git` cannot run or `root` is not in a repository.
pub fn log(root: &Path, most: usize) -> anyhow::Result<Vec<Commit>> {
    let count = format!("--max-count={most}");
    let format = format!("--format=%H{FIELD}%P{FIELD}%an{FIELD}%at{FIELD}%D{FIELD}%s{RECORD}");
    let args = [
        "log",
        "--no-color",
        // Children before parents, which the graph's lanes need.
        "--date-order",
        "--decorate-refs=refs/heads",
        "--decorate-refs=refs/remotes",
        "--decorate-refs=refs/tags",
        &count,
        &format,
        // Before the first commit there is no `HEAD` yet.
        "--ignore-missing",
        "HEAD",
        "--branches",
    ];
    // 128 is also what git exits with before the first commit.
    let output = git(root, &args, &[0, 128])?;
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
            let parents = fields.next()?.split_whitespace().map(str::to_owned);
            let author = fields.next()?.to_owned();
            let seconds = fields.next()?.parse().ok()?;
            let refs = refs(fields.next()?);
            let subject = fields.next()?.to_owned();
            let time = SystemTime::UNIX_EPOCH + Duration::from_secs(seconds);
            Some(Commit {
                id,
                parents: parents.collect(),
                refs,
                subject,
                author,
                time,
            })
        })
        .collect()
}

/// `HEAD -> main, origin/main, tag: v1` as `main`, `origin/main`, `v1`.
fn refs(decorations: &str) -> Vec<String> {
    decorations
        .split(", ")
        .filter(|name| !name.is_empty())
        .map(|name| {
            let name = name.strip_prefix("HEAD -> ").unwrap_or(name);
            name.strip_prefix("tag: ").unwrap_or(name).to_owned()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_are_read_field_by_field() {
        let text = "abcdef123\u{1f}p1 p2\u{1f}Ada\u{1f}60\u{1f}HEAD -> main, tag: v1\u{1f}Fix \
                    it\u{1e}\nfff\u{1f}\u{1f}Bo\u{1f}0\u{1f}\u{1f}a\u{1f}b\u{1e}\n";
        let commits = parse(text);
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].short_id(), "abcdef1");
        assert_eq!(commits[0].parents, ["p1", "p2"]);
        assert_eq!(commits[0].refs, ["main", "v1"]);
        assert_eq!(commits[0].subject, "Fix it");
        assert!(commits[1].parents.is_empty() && commits[1].refs.is_empty());
        assert_eq!(
            commits[0].time,
            SystemTime::UNIX_EPOCH + Duration::from_secs(60)
        );
        assert_eq!(commits[1].subject, "a");
    }
}
