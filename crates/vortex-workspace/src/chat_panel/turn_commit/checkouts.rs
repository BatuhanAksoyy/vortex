//! The checkouts a chat's turn may change: its project's, and the
//! worktrees nested in it that an agent makes to work on a branch of its
//! own.

use std::path::{Path, PathBuf};

use vortex_git::{Fingerprint, Status};

/// The most of the diff the model reads; enough to tell what a turn did.
const MAX_DIFF: usize = 30_000;

/// What the chat changed that is not committed yet.
pub(super) struct Work {
    /// Of the checkout committed in.
    pub(super) status: Status,
    /// It is not the project's own checkout.
    pub(super) nested: bool,
    pub(super) paths: Vec<PathBuf>,
    pub(super) diff: String,
}

/// What each checkout's changed files held, by its top folder; the
/// project's first.
pub(super) struct Snapshot(Vec<(PathBuf, Fingerprint)>);

/// The project's status, then each nested checkout's. One that cannot be
/// read, as while it is being made, is left out.
///
/// # Errors
/// When the project's status cannot be read.
pub(super) fn read(project: &Path) -> anyhow::Result<Vec<Status>> {
    let own = vortex_git::status(project)?;
    let nested = vortex_git::nested_checkouts(project).unwrap_or_default();
    let mut statuses = vec![own];
    statuses.extend(
        nested
            .iter()
            .filter_map(|folder| vortex_git::status(folder).ok()),
    );
    Ok(statuses)
}

impl Snapshot {
    pub(super) fn of(statuses: &[Status]) -> Self {
        let prints = statuses
            .iter()
            .map(|status| (status.root.clone(), Fingerprint::of(status)));
        Self(prints.collect())
    }

    fn print(&self, root: &Path) -> Option<&Fingerprint> {
        let found = self.0.iter().find(|(top, _)| top == root);
        found.map(|(_, print)| print)
    }

    /// Whether `path` was changed in the checkout at `root`.
    pub(super) fn holds(&self, root: &Path, path: &Path) -> bool {
        self.print(root).is_some_and(|print| print.holds(path))
    }

    /// The files each checkout changed since `self`, by its top folder. A
    /// checkout made since counts all it changed. The project's own leave
    /// out the checkouts nested in it, which it sees as new folders.
    pub(super) fn changed_since(&self, later: &Self) -> Vec<(PathBuf, Vec<PathBuf>)> {
        let Some((project, _)) = later.0.first() else {
            return Vec::new();
        };
        let nested: Vec<&Path> = (later.0.iter().skip(1))
            .filter_map(|(root, _)| root.strip_prefix(project).ok())
            .collect();
        let unknown = Fingerprint::default();
        let changed = later.0.iter().map(|(root, print)| {
            let before = self.print(root).unwrap_or(&unknown);
            let mut paths = before.changed_since(print);
            if root == project {
                paths.retain(|path| !nested.iter().any(|inside| path.starts_with(inside)));
            }
            (root.clone(), paths)
        });
        changed.filter(|(_, paths)| !paths.is_empty()).collect()
    }
}

/// The work in the first checkout, the project's before the nested ones,
/// that still has some of the chat's files changed; one commit takes one
/// checkout's. A checkout that cannot be read is passed over.
pub(super) fn first_work(
    project: &Path,
    touched: &[(PathBuf, Vec<PathBuf>)],
) -> anyhow::Result<Option<Work>> {
    let top = vortex_git::status(project)?.root;
    let found = (touched.iter()).find_map(|(root, paths)| work_on(root, paths).ok().flatten());
    Ok(found.map(|work| Work {
        nested: work.status.root != top,
        ..work
    }))
}

/// The chat's `touched` files in the checkout at `root` that are still
/// changed, and their diff; `None` when none is.
fn work_on(root: &Path, touched: &[PathBuf]) -> anyhow::Result<Option<Work>> {
    let status = vortex_git::status(root)?;
    let changes: Vec<_> = (status.changes.iter())
        .filter(|change| touched.contains(&change.path))
        .collect();
    if changes.is_empty() {
        return Ok(None);
    }
    let mut paths: Vec<PathBuf> = changes.iter().map(|change| change.path.clone()).collect();
    paths.dedup();
    let mut diff = String::new();
    for change in changes {
        if diff.len() >= MAX_DIFF {
            break;
        }
        diff.push_str(&vortex_git::diff(&status.root, change)?);
    }
    if let Some((end, _)) = diff.char_indices().nth(MAX_DIFF) {
        diff.truncate(end);
    }
    Ok(Some(Work {
        status,
        nested: false,
        paths,
        diff,
    }))
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;

    fn git(root: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(["-c", "user.name=Vortex", "-c", "user.email=v@example.com"])
            .args(args)
            .current_dir(root)
            .status()
            .expect("git");
        assert!(status.success(), "git {args:?}");
    }

    #[test]
    fn a_turn_in_a_worktree_it_made_is_committed_there() {
        let project = tempfile::tempdir().expect("tempdir");
        let root = project.path();
        std::fs::write(root.join("kept.txt"), "one\n").expect("write");
        git(root, &["init", "-q", "-b", "main"]);
        git(root, &["add", "."]);
        git(root, &["commit", "-q", "-m", "start"]);
        let before = Snapshot::of(&read(root).expect("read"));

        // The turn: the agent makes a worktree in the project and works there.
        let worktree = root.join(".claude/worktrees/fix");
        git(
            root,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "fix",
                &worktree.to_string_lossy(),
            ],
        );
        std::fs::write(worktree.join("new.txt"), "new\n").expect("write");
        let statuses = read(root).expect("read");
        let changed = before.changed_since(&Snapshot::of(&statuses));

        let top = &statuses[1].root;
        assert_eq!(
            changed,
            [(top.clone(), vec![PathBuf::from("new.txt")])],
            "the worktree's file, and not the worktree as a new folder of the project"
        );
        let work = first_work(root, &changed).expect("read").expect("work");
        assert!(work.nested, "not the project's own checkout");
        assert_eq!(&work.status.root, top);
        assert_eq!(work.paths, [PathBuf::from("new.txt")]);
        assert!(work.diff.contains("+new"));
    }
}
