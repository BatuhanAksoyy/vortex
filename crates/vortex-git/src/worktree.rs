//! Worktrees Vortex makes for a chat, so its agent works on a branch of its
//! own beside the user's checkout. The branch it started from is kept in
//! the branch's own git config, which also marks the worktree as Vortex's,
//! and goes when the branch is deleted.

use std::path::{Path, PathBuf};

use anyhow::bail;

use crate::{git, line};

/// Before each worktree's branch name.
const BRANCH_PREFIX: &str = "vortex/";
/// The config key, under `branch.<name>`, that holds the base branch.
const BASE_KEY: &str = "vortexBase";
/// The same two from when Vortex was called Kiln, so its worktrees stay
/// known.
const OLD_BRANCH_PREFIX: &str = "kiln/";
const OLD_BASE_KEY: &str = "kilnBase";

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
/// branch `vortex/<name>` from the commit checked out there. Returns it and
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

/// The Vortex worktree `folder` lies in; `None` for any other folder.
///
/// # Errors
/// When `git` cannot run.
pub fn worktree_of(folder: &Path) -> anyhow::Result<Option<Worktree>> {
    let Ok(path) = line(folder, "--show-toplevel") else {
        return Ok(None);
    };
    let path = PathBuf::from(path);
    let git_dir = PathBuf::from(line(&path, "--absolute-git-dir")?);
    let common = common_dir(&path)?;
    // The main checkout's git folder is the common one; a linked
    // worktree's is a folder inside it.
    let Some(main) = common.parent().filter(|_| git_dir != common) else {
        return Ok(None);
    };
    let main = main.to_path_buf();
    let Ok(branch) = current_branch(&path) else {
        return Ok(None);
    };
    let key = if branch.starts_with(BRANCH_PREFIX) {
        BASE_KEY
    } else if branch.starts_with(OLD_BRANCH_PREFIX) {
        OLD_BASE_KEY
    } else {
        return Ok(None);
    };
    let key = format!("branch.{branch}.{key}");
    let base = git(&path, &["config", "--get", &key], &[0, 1])?;
    let base = String::from_utf8_lossy(&base.stdout).trim().to_owned();
    if base.is_empty() {
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

/// Whether `folder` is a checkout of the repository `project` is in: its
/// main checkout or any of its worktrees, a folder inside one too.
///
/// # Errors
/// When `git` cannot run.
pub fn same_repository(project: &Path, folder: &Path) -> anyhow::Result<bool> {
    let (Ok(ours), Ok(theirs)) = (common_dir(project), common_dir(folder)) else {
        return Ok(false);
    };
    Ok(ours.canonicalize()? == theirs.canonicalize()?)
}

/// The other checkouts of the repository `project` is in that lie inside
/// `project`, such as the worktrees an agent makes under it, each by its
/// top folder. Worktrees elsewhere, like Vortex's own for other chats, are
/// left out.
///
/// # Errors
/// When `git` cannot run or `project` is not in a repository.
pub fn nested_checkouts(project: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let inside = project.canonicalize()?;
    let listed = git(project, &["worktree", "list", "--porcelain"], &[0])?;
    let listed = String::from_utf8_lossy(&listed.stdout);
    let checkouts = (listed.lines())
        .filter_map(|line| line.strip_prefix("worktree "))
        .map(PathBuf::from)
        .filter(|path| {
            // A worktree whose folder is gone is still listed until pruned.
            let found = path.canonicalize().ok();
            found.is_some_and(|found| found != inside && found.starts_with(&inside))
        });
    Ok(checkouts.collect())
}

/// The git folder every worktree of `folder`'s repository shares.
fn common_dir(folder: &Path) -> anyhow::Result<PathBuf> {
    let common = git(
        folder,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        &[0],
    )?;
    Ok(PathBuf::from(
        String::from_utf8_lossy(&common.stdout).trim(),
    ))
}

/// Takes the worktree and its branch away when nothing was done on them:
/// no change in its folder, not even a new file, and no commit its base
/// lacks. Returns whether they went.
///
/// # Errors
/// When `git` fails.
pub fn remove_unused_worktree(worktree: &Worktree) -> anyhow::Result<bool> {
    if !worktree.path.exists() || !crate::status(&worktree.path)?.changes.is_empty() {
        return Ok(false);
    }
    let range = format!("{}..{}", worktree.base, worktree.branch);
    let ahead = git(&worktree.main, &["rev-list", "--count", &range], &[0])?;
    if String::from_utf8_lossy(&ahead.stdout).trim() != "0" {
        return Ok(false);
    }
    // Without --force git itself refuses a worktree that holds work. The
    // branch has nothing its base lacks, though `-d` would also want it
    // merged into whatever the main checkout has checked out.
    let path = worktree.path.to_string_lossy();
    git(&worktree.main, &["worktree", "remove", &path], &[0])?;
    git(&worktree.main, &["branch", "-D", &worktree.branch], &[0])?;
    Ok(true)
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

#[cfg(test)]
mod tests {
    use crate::tests::repository;

    #[test]
    fn a_worktree_is_a_checkout_of_the_same_repository() {
        let dir = repository();
        let root = dir.path();
        std::fs::create_dir(root.join("sub")).expect("mkdir");
        let into = tempfile::tempdir().expect("temp dir");
        let path = into.path().join("four");
        let target = path.to_string_lossy();
        super::git(
            root,
            &["worktree", "add", "-q", "-b", "four", &target],
            &[0],
        )
        .expect("add");
        assert!(super::same_repository(&root.join("sub"), &path).expect("read"));
        assert!(super::same_repository(&path, root).expect("read"));
        let other = repository();
        assert!(!super::same_repository(root, other.path()).expect("read"));
        let outside = super::same_repository(root, into.path()).expect("read");
        assert!(!outside, "no repository");
    }

    #[test]
    fn only_the_worktrees_inside_the_project_are_nested_in_it() {
        let dir = repository();
        let root = dir.path();
        let add = |path: &std::path::Path, branch: &str| {
            let target = path.to_string_lossy();
            let args = ["worktree", "add", "-q", "-b", branch, &target];
            super::git(root, &args, &[0]).expect("add");
        };
        let inside = root.join(".claude/worktrees/fix");
        add(&inside, "fix");
        let elsewhere = tempfile::tempdir().expect("temp dir");
        add(&elsewhere.path().join("other"), "other");
        let gone = root.join("gone");
        add(&gone, "gone");
        std::fs::remove_dir_all(&gone).expect("remove");

        let nested = super::nested_checkouts(root).expect("read");
        let nested: Vec<_> = nested
            .iter()
            .map(|path| path.canonicalize().expect("real"))
            .collect();
        assert_eq!(nested, [inside.canonicalize().expect("real")]);
        assert_eq!(
            super::nested_checkouts(&inside).expect("read"),
            Vec::<std::path::PathBuf>::new()
        );
    }
}
