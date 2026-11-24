//! How many lines each change adds and removes.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::{Area, Change, git};

/// Lines added and removed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub added: usize,
    pub removed: usize,
}

pub type CountMap = HashMap<(Area, PathBuf), Counts>;

/// Untracked files are read to be counted; a large one, or one of very
/// many, is left without a count.
const MAX_COUNTED_BYTES: u64 = 1024 * 1024;
const MAX_COUNTED_FILES: usize = 500;

pub fn read(root: &Path, changes: &[Change]) -> anyhow::Result<CountMap> {
    let mut counts = CountMap::new();
    for (area, option) in [(Area::Staged, Some("--cached")), (Area::Unstaged, None)] {
        if !changes.iter().any(|change| change.area == area) {
            continue;
        }
        let mut args = vec!["diff", "--numstat", "-z", "--no-ext-diff", "--find-renames"];
        args.extend(option);
        let output = git(root, &args, &[0])?;
        let found = parse(&String::from_utf8_lossy(&output.stdout));
        counts.extend(found.into_iter().map(|(path, count)| ((area, path), count)));
    }
    let untracked = changes
        .iter()
        .filter(|change| change.area == Area::Untracked);
    for change in untracked.take(MAX_COUNTED_FILES) {
        if let Some(added) = lines_in(&root.join(&change.path)) {
            let key = (Area::Untracked, change.path.clone());
            counts.insert(key, Counts { added, removed: 0 });
        }
    }
    Ok(counts)
}

/// `added<tab>removed<tab>path` records, whose path is empty and followed by
/// the old and the new one for a rename. Binary files count `-` and are
/// left out.
pub fn parse(output: &str) -> Vec<(PathBuf, Counts)> {
    let mut found = Vec::new();
    let mut records = output.split('\0');
    while let Some(record) = records.next() {
        let mut fields = record.splitn(3, '\t');
        let (Some(added), Some(removed), Some(path)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let path = if path.is_empty() {
            records.nth(1).unwrap_or_default()
        } else {
            path
        };
        if let (Ok(added), Ok(removed)) = (added.parse(), removed.parse()) {
            found.push((PathBuf::from(path), Counts { added, removed }));
        }
    }
    found
}

fn lines_in(path: &Path) -> Option<usize> {
    let metadata = std::fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_COUNTED_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    (!bytes.contains(&0)).then(|| String::from_utf8_lossy(&bytes).lines().count())
}
