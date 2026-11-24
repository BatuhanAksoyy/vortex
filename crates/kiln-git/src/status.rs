//! `git status --porcelain=v1 -z --branch`, read into changes.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::counts::{CountMap, Counts};

/// The branch checked out and every change in the repository.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    /// The repository's top folder.
    pub root: PathBuf,
    /// Where the project lies in the repository; empty at its top.
    pub(crate) prefix: PathBuf,
    /// `None` while no branch is checked out, as during a rebase.
    pub branch: Option<String>,
    /// Commits the upstream branch lacks, and commits it has on top.
    pub ahead: usize,
    pub behind: usize,
    /// A file staged and changed again since shows once in each area.
    pub changes: Vec<Change>,
    pub(crate) counts: CountMap,
}

impl Status {
    /// The lines `change` adds and removes; `None` for a binary file.
    #[must_use]
    pub fn counts(&self, change: &Change) -> Option<Counts> {
        let key = (change.area, change.path.clone());
        self.counts.get(&key).copied()
    }

    /// The changed files and the folders holding them, by path in the
    /// project. A folder counts as untracked only when all its changes are.
    #[must_use]
    pub fn marks(&self) -> HashMap<PathBuf, Kind> {
        let mut marks: HashMap<PathBuf, Kind> = HashMap::new();
        for change in &self.changes {
            let Ok(path) = change.path.strip_prefix(&self.prefix) else {
                continue;
            };
            // Listed after the staged change, so the newest one wins.
            marks.insert(path.to_path_buf(), change.kind);
            for folder in path.ancestors().skip(1) {
                marks
                    .entry(folder.to_path_buf())
                    .and_modify(|kind| *kind = folder_kind(*kind, change.kind))
                    .or_insert_with(|| folder_kind(change.kind, change.kind));
            }
        }
        marks
    }
}

fn folder_kind(one: Kind, other: Kind) -> Kind {
    match (one, other) {
        (Kind::Conflicted, _) | (_, Kind::Conflicted) => Kind::Conflicted,
        (Kind::Untracked, Kind::Untracked) => Kind::Untracked,
        _ => Kind::Modified,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    /// Relative to the repository root.
    pub path: PathBuf,
    /// Where a renamed or copied file came from.
    pub original: Option<PathBuf>,
    pub area: Area,
    pub kind: Kind,
}

/// Where a change waits: in an unresolved merge, in the index, in the
/// working tree, or in a file git does not track yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Area {
    Conflicted,
    Staged,
    Unstaged,
    Untracked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    /// Both sides of a merge changed it; it waits to be resolved.
    Conflicted,
    Untracked,
}

impl Kind {
    /// The letter `git status --short` shows it with.
    #[must_use]
    pub fn letter(self) -> char {
        match self {
            Self::Added => 'A',
            Self::Modified => 'M',
            Self::Deleted => 'D',
            Self::Renamed => 'R',
            Self::Copied => 'C',
            Self::TypeChanged => 'T',
            Self::Conflicted => '!',
            Self::Untracked => 'U',
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Added => "Added",
            Self::Modified => "Modified",
            Self::Deleted => "Deleted",
            Self::Renamed => "Renamed",
            Self::Copied => "Copied",
            Self::TypeChanged => "Type changed",
            Self::Conflicted => "Conflicted",
            Self::Untracked => "Untracked",
        }
    }

    fn from_letter(letter: char) -> Option<Self> {
        Some(match letter {
            'A' => Self::Added,
            'M' => Self::Modified,
            'D' => Self::Deleted,
            'R' => Self::Renamed,
            'C' => Self::Copied,
            'T' => Self::TypeChanged,
            _ => return None,
        })
    }
}

/// The states `git status` marks an unresolved merge with.
const CONFLICTS: [&str; 7] = ["DD", "AU", "UD", "UA", "DU", "AA", "UU"];

pub fn parse(output: &str) -> Status {
    let mut status = Status::default();
    let mut records = output.split('\0').filter(|record| !record.is_empty());
    while let Some(record) = records.next() {
        if let Some(branch) = record.strip_prefix("## ") {
            status.branch = branch_name(branch);
            status.ahead = number_after(branch, "ahead ");
            status.behind = number_after(branch, "behind ");
            continue;
        }
        let (Some(code), Some(path)) = (record.get(..2), record.get(3..)) else {
            continue;
        };
        let path = PathBuf::from(path);
        // A rename or copy is followed by the path it came from.
        let original = code
            .contains(['R', 'C'])
            .then(|| records.next().map(PathBuf::from))
            .flatten();
        status
            .changes
            .extend(changes(code, &path, original.as_ref()));
    }
    status
}

/// The branch in a `## ` line: `main...origin/main [ahead 1]`, `No commits
/// yet on main`, or `HEAD (no branch)` when detached.
fn branch_name(line: &str) -> Option<String> {
    let line = line.strip_prefix("No commits yet on ").unwrap_or(line);
    if line.starts_with("HEAD (no branch)") {
        return None;
    }
    let name = line.split("...").next().unwrap_or(line);
    let name = name.split(' ').next().unwrap_or(name);
    Some(name.to_owned())
}

/// The number after `word` in the `[ahead 1, behind 2]` that ends a line.
fn number_after(line: &str, word: &str) -> usize {
    let bracket = line.rsplit_once(" [").map_or("", |(_, bracket)| bracket);
    let digits = bracket.split_once(word).map_or("", |(_, rest)| rest);
    let end = digits.find(|char: char| !char.is_ascii_digit());
    digits[..end.unwrap_or(digits.len())].parse().unwrap_or(0)
}

fn changes(code: &str, path: &Path, original: Option<&PathBuf>) -> Vec<Change> {
    let change = |area, kind| Change {
        path: path.to_path_buf(),
        original: original.cloned(),
        area,
        kind,
    };
    if code == "??" {
        return vec![change(Area::Untracked, Kind::Untracked)];
    }
    if CONFLICTS.contains(&code) {
        return vec![change(Area::Conflicted, Kind::Conflicted)];
    }
    let mut letters = code.chars();
    let staged = letters.next().and_then(Kind::from_letter);
    let unstaged = letters.next().and_then(Kind::from_letter);
    let staged = staged.map(|kind| change(Area::Staged, kind));
    let unstaged = unstaged.map(|kind| Change {
        // Renames live in the index; the working tree only edits the file.
        original: None,
        ..change(Area::Unstaged, kind)
    });
    staged.into_iter().chain(unstaged).collect()
}
