//! Worktrees Kiln makes for a chat, so its agent works on a branch of its
//! own beside the user's checkout. The branch it started from is kept in
//! the branch's own git config, which also marks the worktree as Kiln's,
//! and goes when the branch is deleted.

use std::path::{Path, PathBuf};

use anyhow::bail;

use crate::{git, line};

/// Before each worktree's branch name.
const BRANCH_PREFIX: &str = "kiln/";
/// The config key, under `branch.<name>`, that holds the base branch.
const BASE_KEY: &str = "kilnBase";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Worktree {
    /// The worktree's top folder.
    pub path: PathBuf,
    pub branch: String,
    /// The branch it started from, which it merges back into.
    pub base: String,
    /// The top folder of the checkout it was made from.
    pub main: PathBuf,
}

/// Makes a worktree of the repository `project` is in, at `path`, on a new
/// branch `kiln/<name>` from the commit checked out there. Returns it and
/// the folder in it that matches `project`.
///
/// # Errors
/// When no branch is checked out, the branch or folder exists, or `git`
/// fails.
pub fn create_worktree(
    project: &Path,
    path: &Path,
    name: &str,
) -> anyhow::Result<(Worktree, PathBuf)> {
    let main = PathBuf::from(line(project, "--show-toplevel")?);
    let prefix = PathBuf::from(line(project, "--show-prefix")?);
    let Ok(base) = current_branch(&main) else {
        bail!("Check out a branch first; a worktree starts from one.");
    };
    let branch = format!("{BRANCH_PREFIX}{name}");
    let target = path.to_string_lossy();
    git(
        &main,
        &["worktree", "add", "--quiet", "-b", &branch, &target],
        &[0],
    )?;
    let key = format!("branch.{branch}.{BASE_KEY}");
    git(&main, &["config", &key, &base], &[0])?;
    let folder = path.join(prefix);
    let worktree = Worktree {
        path: path.to_path_buf(),
        branch,
        base,
        main,
    };
    Ok((worktree, folder))
}

/// The Kiln worktree `folder` lies in; `None` for any other folder.
///
/// # Errors
/// When `git` cannot run.
pub fn worktree_of(folder: &Path) -> anyhow::Result<Option<Worktree>> {
    let Ok(path) = line(folder, "--show-toplevel") else {
        return Ok(None);
    };
    let path = PathBuf::from(path);
    let git_dir = PathBuf::from(line(&path, "--absolute-git-dir")?);
    let common = git(
        &path,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        &[0],
    )?;
    let common = PathBuf::from(String::from_utf8_lossy(&common.stdout).trim());
    // The main checkout's git folder is the common one; a linked
    // worktree's is a folder inside it.
    let Some(main) = common.parent().filter(|_| git_dir != common) else {
        return Ok(None);
    };
    let main = main.to_path_buf();
    let Ok(branch) = current_branch(&path) else {
        return Ok(None);
    };
    let key = format!("branch.{branch}.{BASE_KEY}");
    let base = git(&path, &["config", "--get", &key], &[0, 1])?;
    let base = String::from_utf8_lossy(&base.stdout).trim().to_owned();
    if base.is_empty() || !branch.starts_with(BRANCH_PREFIX) {
        return Ok(None);
    }
    Ok(Some(Worktree {
        path,
        branch,
        base,
        main,
    }))
}

/// Merges the worktree's branch into its base in the main checkout, then
/// takes the worktree and its branch away.
///
/// # Errors
/// When the worktree has changes not committed, the main checkout is on
/// another branch, the merge conflicts (and is undone), or `git` fails.
pub fn merge_worktree(worktree: &Worktree) -> anyhow::Result<()> {
    if !crate::status(&worktree.path)?.changes.is_empty() {
        bail!("Commit or discard the worktree's changes first.");
    }
    let on = current_branch(&worktree.main).unwrap_or_default();
    if on != worktree.base {
        bail!(
            "{} has {on} checked out; check out {} there to merge into it.",
            worktree.main.display(),
            worktree.base
        );
    }
    let merged = git(
        &worktree.main,
        &["merge", "--no-edit", "--quiet", &worktree.branch],
        &[0],
    );
    if let Err(error) = merged {
        let _ = git(&worktree.main, &["merge", "--abort"], &[0, 128]);
        bail!("The merge did not go through, so it was undone: {error:#}");
    }
    remove(worktree, false)
}

/// Takes the worktree and its branch away, with whatever is on them.
///
/// # Errors
/// When `git` fails.
pub fn discard_worktree(worktree: &Worktree) -> anyhow::Result<()> {
    remove(worktree, true)
}

fn remove(worktree: &Worktree, force: bool) -> anyhow::Result<()> {
    let path = worktree.path.to_string_lossy();
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(&path);
    git(&worktree.main, &args, &[0])?;
    let delete = if force { "-D" } else { "-d" };
    git(&worktree.main, &["branch", delete, &worktree.branch], &[0]).map(drop)
}

fn current_branch(root: &Path) -> anyhow::Result<String> {
    let output = git(root, &["symbolic-ref", "--quiet", "--short", "HEAD"], &[0])?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
