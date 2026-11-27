//! The local branches, and moving between them.

use std::path::Path;

use anyhow::bail;

use crate::git;

/// Between a branch's fields; never in a branch name.
const FIELD: char = '\u{1f}';

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Branch {
    pub name: String,
    /// Checked out in this working tree.
    pub current: bool,
    /// The branch it tracks, such as `origin/main`.
    pub upstream: Option<String>,
}

/// The local branches, the most recently committed to first.
///
/// # Errors
/// When `git` cannot run or `root` is not in a repository.
pub fn branches(root: &Path) -> anyhow::Result<Vec<Branch>> {
    let format = format!("--format=%(HEAD){FIELD}%(refname:short){FIELD}%(upstream:short)");
    let args = [
        "for-each-ref",
        "--sort=-committerdate",
        &format,
        "refs/heads",
    ];
    let output = git(root, &args, &[0])?;
    Ok(parse(&String::from_utf8_lossy(&output.stdout)))
}

fn parse(output: &str) -> Vec<Branch> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split(FIELD);
            let (head, name, upstream) = (fields.next()?, fields.next()?, fields.next()?);
            Some(Branch {
                name: name.to_owned(),
                current: head == "*",
                upstream: (!upstream.is_empty()).then(|| upstream.to_owned()),
            })
        })
        .collect()
}

/// Checks out branch `name`. Changes in the working tree go along, unless
/// the branch changes the same files; git then refuses and says which.
///
/// # Errors
/// When the switch would lose changes, or `git` cannot run.
pub fn switch(root: &Path, name: &str) -> anyhow::Result<()> {
    git(root, &["switch", "--quiet", name], &[0]).map(drop)
}

/// Starts branch `name` at the current commit and checks it out, keeping
/// the changes in the working tree.
///
/// # Errors
/// When the name is taken or not a valid branch name, or `git` cannot run.
pub fn create_branch(root: &Path, name: &str) -> anyhow::Result<()> {
    let name = name.trim();
    if name.is_empty() {
        bail!("A branch needs a name.");
    }
    // `--` would end the options, but `switch` takes the name before it.
    if name.starts_with('-') {
        bail!("A branch name cannot start with \"-\".");
    }
    git(root, &["switch", "--quiet", "--create", name], &[0]).map(drop)
}

#[cfg(test)]
mod tests {
    use super::{Branch, parse};

    #[test]
    fn reads_each_branch_and_which_is_checked_out() {
        let output = "*\u{1f}main\u{1f}origin/main\n \u{1f}feature/x\u{1f}\n";
        assert_eq!(
            parse(output),
            [
                Branch {
                    name: "main".into(),
                    current: true,
                    upstream: Some("origin/main".into()),
                },
                Branch {
                    name: "feature/x".into(),
                    current: false,
                    upstream: None,
                },
            ]
        );
    }
}
