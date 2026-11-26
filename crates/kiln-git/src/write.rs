//! What Kiln changes in a repository: the index and new commits. Each runs
//! the user's own `git`, so their hooks and identity apply.

use std::{
    io::Write as _,
    path::Path,
    process::{Command, Stdio},
};

use anyhow::{Context as _, bail};

use crate::{Area, Change, git};

/// Stages `changes` as they are in the working tree, removals too.
///
/// # Errors
/// When `git` cannot run or fails.
pub fn stage(root: &Path, changes: &[Change]) -> anyhow::Result<()> {
    let mut args = vec!["add", "--all", "--"];
    let paths = paths(changes);
    args.extend(paths.iter().map(String::as_str));
    git(root, &args, &[0]).map(drop)
}

/// Takes `changes` out of the index, keeping them in the working tree.
///
/// # Errors
/// When `git` cannot run or fails.
pub fn unstage(root: &Path, changes: &[Change]) -> anyhow::Result<()> {
    // `reset` also works before the first commit, where `restore` has no
    // HEAD to restore from.
    let mut args = vec!["reset", "--quiet", "--"];
    let paths = paths(changes);
    args.extend(paths.iter().map(String::as_str));
    git(root, &args, &[0]).map(drop)
}

/// Commits what is staged with `message`, returning the new commit's id.
///
/// # Errors
/// When nothing is staged, a hook refuses, or `git` cannot run.
pub fn commit(root: &Path, message: &str) -> anyhow::Result<String> {
    let mut child = Command::new("git")
        .args(["commit", "--quiet", "--file=-", "--cleanup=strip"])
        .env("LC_ALL", "C")
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("cannot run git")?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(message.as_bytes())
            .context("cannot write the message")?;
    }
    let output = child.wait_with_output().context("cannot run git")?;
    if !output.status.success() {
        // Hooks often explain a refusal on stdout.
        let said = [&output.stderr, &output.stdout].map(|bytes| String::from_utf8_lossy(bytes));
        let said: Vec<&str> = said
            .iter()
            .map(|text| text.trim())
            .filter(|text| !text.is_empty())
            .collect();
        bail!("{}", said.join("\n"));
    }
    let head = git(root, &["rev-parse", "HEAD"], &[0])?;
    Ok(String::from_utf8_lossy(&head.stdout).trim().to_owned())
}

/// The paths a change touches, where a staged rename came from as well.
fn paths(changes: &[Change]) -> Vec<String> {
    let mut paths = Vec::new();
    for change in changes {
        if change.area == Area::Staged {
            paths.extend(
                change
                    .original
                    .iter()
                    .map(|path| path.to_string_lossy().into_owned()),
            );
        }
        paths.push(change.path.to_string_lossy().into_owned());
    }
    paths
}
