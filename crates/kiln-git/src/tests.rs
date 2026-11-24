use std::{fs, path::Path, path::PathBuf, process::Command};

use super::{Area, Change, Counts, Kind, counts, diff, status, status::parse};

fn run(root: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(["-c", "user.name=Kiln", "-c", "user.email=kiln@example.com"])
        .args(args)
        .current_dir(root)
        .status()
        .expect("run git");
    assert!(status.success(), "git {args:?}");
}

/// A repository on `main` with `kept.txt` and `old.txt` committed.
fn repository() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp dir");
    run(dir.path(), &["init", "-q", "-b", "main"]);
    fs::write(dir.path().join("kept.txt"), "one\ntwo\n").expect("write");
    fs::write(dir.path().join("old.txt"), "moved\n").expect("write");
    run(dir.path(), &["add", "."]);
    run(dir.path(), &["commit", "-q", "-m", "start"]);
    dir
}

fn change(path: &str, area: Area, kind: Kind) -> Change {
    Change {
        path: PathBuf::from(path),
        original: None,
        area,
        kind,
    }
}

#[test]
fn reads_the_branch_in_its_forms() {
    let branch = |line: &str| parse(&format!("## {line}\0")).branch;
    assert_eq!(
        branch("main...origin/main [ahead 1]").as_deref(),
        Some("main")
    );
    assert_eq!(branch("feature/x").as_deref(), Some("feature/x"));
    assert_eq!(branch("No commits yet on main").as_deref(), Some("main"));
    assert_eq!(branch("HEAD (no branch)"), None);
}

#[test]
fn a_file_staged_and_changed_again_shows_in_both_areas() {
    let status = parse("MM src/lib.rs\0?? new.txt\0UU both.txt\0");
    assert_eq!(
        status.changes,
        [
            change("src/lib.rs", Area::Staged, Kind::Modified),
            change("src/lib.rs", Area::Unstaged, Kind::Modified),
            change("new.txt", Area::Untracked, Kind::Untracked),
            change("both.txt", Area::Conflicted, Kind::Conflicted),
        ]
    );
}

#[test]
fn a_rename_keeps_where_it_came_from() {
    let status = parse("R  new name.txt\0old.txt\0 M kept.txt\0");
    assert_eq!(status.changes.len(), 2);
    assert_eq!(status.changes[0].path, PathBuf::from("new name.txt"));
    assert_eq!(status.changes[0].original, Some(PathBuf::from("old.txt")));
    assert_eq!(status.changes[0].kind, Kind::Renamed);
    assert_eq!(
        status.changes[1],
        change("kept.txt", Area::Unstaged, Kind::Modified)
    );
}

#[test]
fn reads_a_real_repository_and_its_diffs() {
    let dir = repository();
    let root = dir.path();
    fs::write(root.join("kept.txt"), "one\nthree\n").expect("write");
    fs::write(root.join("ünïcode.txt"), "fresh\n").expect("write");
    run(root, &["mv", "old.txt", "new.txt"]);

    let found = status(root).expect("status");
    assert_eq!(found.branch.as_deref(), Some("main"));
    let kept = change("kept.txt", Area::Unstaged, Kind::Modified);
    assert!(found.changes.contains(&kept), "{found:?}");
    let untracked = change("ünïcode.txt", Area::Untracked, Kind::Untracked);
    assert!(found.changes.contains(&untracked), "{found:?}");
    let renamed = found
        .changes
        .iter()
        .find(|change| change.kind == Kind::Renamed)
        .expect("the rename");

    let patch = diff(&found.root, &kept).expect("diff");
    assert!(
        patch.contains("-two") && patch.contains("+three"),
        "{patch}"
    );
    let patch = diff(&found.root, &untracked).expect("diff");
    assert!(patch.contains("+fresh"), "{patch}");
    let patch = diff(&found.root, renamed).expect("diff");
    assert!(patch.contains("rename from old.txt"), "{patch}");
}

#[test]
fn a_folder_outside_a_repository_is_an_error() {
    let dir = tempfile::tempdir().expect("temp dir");
    assert!(status(dir.path()).is_err());
}

#[test]
fn reads_how_far_the_branch_is_from_its_upstream() {
    let read = parse("## main...origin/main [ahead 12, behind 3]\0");
    assert_eq!((read.ahead, read.behind), (12, 3));
    let read = parse("## main...origin/main\0");
    assert_eq!((read.ahead, read.behind), (0, 0));
}

#[test]
fn counts_lines_past_renames_and_binary_files() {
    let found = counts::parse("1\t2\ta b.rs\0-\t-\tlogo.png\x000\t4\t\0old.rs\0new.rs\0");
    let count = |added, removed| Counts { added, removed };
    assert_eq!(
        found,
        [
            (PathBuf::from("a b.rs"), count(1, 2)),
            (PathBuf::from("new.rs"), count(0, 4)),
        ]
    );
}

#[test]
fn a_project_below_the_repositorys_top_reads_the_same_changes() {
    let dir = repository();
    let project = dir.path().join("app/src");
    fs::create_dir_all(&project).expect("mkdir");
    fs::write(project.join("[x].rs"), "one\ntwo\n").expect("write");
    fs::write(dir.path().join("kept.txt"), "one\n").expect("write");

    let found = status(&project).expect("status");
    let new = change("app/src/[x].rs", Area::Untracked, Kind::Untracked);
    assert!(found.changes.contains(&new), "{found:?}");
    assert_eq!(
        found.counts(&new),
        Some(Counts {
            added: 2,
            removed: 0
        })
    );
    let kept = change("kept.txt", Area::Unstaged, Kind::Modified);
    assert_eq!(
        found.counts(&kept),
        Some(Counts {
            added: 0,
            removed: 1
        })
    );
    assert!(diff(&found.root, &kept).expect("diff").contains("-two"));
    let marks = found.marks();
    assert_eq!(marks.get(Path::new("[x].rs")), Some(&Kind::Untracked));
    assert!(!marks.contains_key(Path::new("kept.txt")), "{marks:?}");
}

#[test]
fn folders_are_marked_by_what_they_hold() {
    let marks = parse(" M src/a.rs\0?? src/new.rs\0?? docs/x.md\0").marks();
    assert_eq!(marks.get(Path::new("src")), Some(&Kind::Modified));
    assert_eq!(marks.get(Path::new("docs")), Some(&Kind::Untracked));
    assert_eq!(marks.get(Path::new("src/new.rs")), Some(&Kind::Untracked));
}

#[test]
fn a_conflict_shows_its_diff_against_the_current_branch() {
    let dir = repository();
    let root = dir.path();
    run(root, &["checkout", "-q", "-b", "other"]);
    fs::write(root.join("kept.txt"), "theirs\n").expect("write");
    run(root, &["commit", "-qam", "theirs"]);
    run(root, &["checkout", "-q", "main"]);
    fs::write(root.join("kept.txt"), "ours\n").expect("write");
    run(root, &["commit", "-qam", "ours"]);
    let merge = Command::new("git")
        .args(["-c", "user.name=K", "-c", "user.email=k@example.com"])
        .args(["merge", "-q", "other"])
        .current_dir(root)
        .output()
        .expect("run git");
    assert!(!merge.status.success(), "the merge conflicts");

    let found = status(root).expect("status");
    let both = change("kept.txt", Area::Conflicted, Kind::Conflicted);
    assert_eq!(found.changes, std::slice::from_ref(&both));
    assert!(diff(&found.root, &both).expect("diff").contains("+theirs"));
}

#[test]
fn reads_a_files_text_in_the_last_commit() {
    let dir = repository();
    let path = dir.path().join("kept.txt");
    let committed = fs::read_to_string(&path).expect("read");
    fs::write(&path, "changed\n").expect("write");
    assert_eq!(crate::base_text(&path), Some(committed));
    assert_eq!(crate::base_text(&dir.path().join("none.txt")), None);
}
