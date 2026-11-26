//! The git CLI, as Kiln reads it: the branch, what changed in the working
//! tree and the index, and each change's diff. It also stages, commits and
//! switches branches. Kiln runs the user's own `git`, so their
//! configuration applies.

mod branches;
mod counts;
mod fingerprint;
mod lines;
mod log;
mod message;
mod status;
mod worktree;
mod write;

use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};

use anyhow::{Context as _, bail};

pub use branches::{Branch, branches, create_branch, switch};
pub use counts::Counts;
pub use fingerprint::Fingerprint;
pub use lines::{LineChange, LineKind, base_text, line_changes, patch};
pub use log::{Commit, log, show};
pub use message::MessageRules;
pub use status::{Area, Change, Kind, Status};
pub use worktree::{Worktree, create_worktree, discard_worktree, merge_worktree, worktree_of};
pub use write::{commit, commit_paths, push, stage, unstage};

/// A diff the parser can read whatever the user's `diff.*` settings are.
const PLAIN_DIFF: [&str; 7] = [
    "diff",
    "--no-color",
    "--no-ext-diff",
    "--no-textconv",
    "--find-renames",
    "--src-prefix=a/",
    "--dst-prefix=b/",
];

/// The branch and every change in the repository `project` lies in.
///
/// # Errors
/// When `git` cannot run or `project` is not in a repository.
pub fn status(project: &Path) -> anyhow::Result<Status> {
    let root = PathBuf::from(line(project, "--show-toplevel")?);
    let prefix = PathBuf::from(line(project, "--show-prefix")?);
    let args = [
        "status",
        "--porcelain=v1",
        "-z",
        "--branch",
        "--untracked-files=all",
    ];
    let output = git(&root, &args, &[0])?;
    let mut status = status::parse(&String::from_utf8_lossy(&output.stdout));
    status.counts = counts::read(&root, &status.changes)?;
    status.root = root;
    status.prefix = prefix;
    Ok(status)
}

/// The patch of `change` in the repository at `root`, as `git diff` prints
/// it.
///
/// # Errors
/// When `git` cannot run or fails.
pub fn diff(root: &Path, change: &Change) -> anyhow::Result<String> {
    let path = change.path.to_string_lossy();
    let original = change.original.as_ref().map(|path| path.to_string_lossy());
    let mut args = PLAIN_DIFF.to_vec();
    let mut ok: &[i32] = &[0];
    match change.area {
        Area::Conflicted => args.push("HEAD"),
        Area::Staged => args.push("--cached"),
        Area::Unstaged => {}
        Area::Untracked => {
            args.push("--no-index");
            // It exits with 1 when the files differ, which they do.
            ok = &[0, 1];
        }
    }
    args.push("--");
    if change.area == Area::Untracked {
        args.push("/dev/null");
    }
    args.extend(original.as_deref());
    args.push(&path);
    let output = git(root, &args, ok)?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn line(project: &Path, option: &str) -> anyhow::Result<String> {
    let output = git(project, &["rev-parse", option], &[0])?;
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    Ok(text.trim_end_matches(['\r', '\n']).to_owned())
}

/// Runs `git` in `folder`, accepting the exit codes in `ok`.
fn git(folder: &Path, args: &[&str], ok: &[i32]) -> anyhow::Result<Output> {
    let output = Command::new("git")
        // Without optional locks a read never takes `index.lock`, which
        // would make an agent's own `git` call in the checkout fail.
        .args(["--no-optional-locks", "--literal-pathspecs"])
        .args(args)
        .env("LC_ALL", "C")
        .current_dir(folder)
        .output()
        .context("cannot run git")?;
    if output.status.code().is_some_and(|code| ok.contains(&code)) {
        return Ok(output);
    }
    let message = String::from_utf8_lossy(&output.stderr);
    bail!("{}", message.trim())
}

#[cfg(test)]
mod tests;
