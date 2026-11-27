//! What the changed files hold at one moment, to tell later which of them
//! changed since: those are what a turn of an agent's work changed.

use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash as _, Hasher as _},
    path::PathBuf,
};

use crate::Status;

/// Each changed file's contents, as a hash, by its path from the
/// repository's top; a removed file has none.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fingerprint(HashMap<PathBuf, Option<u64>>);

impl Fingerprint {
    /// Reads the files `status` lists as changed.
    #[must_use]
    pub fn of(status: &Status) -> Self {
        let files = status.changes.iter().map(|change| {
            let contents = std::fs::read(status.root.join(&change.path)).ok();
            let hash = contents.map(|bytes| {
                let mut hasher = DefaultHasher::new();
                bytes.hash(&mut hasher);
                hasher.finish()
            });
            (change.path.clone(), hash)
        });
        Self(files.collect())
    }

    /// Whether `path` was among the changed files.
    #[must_use]
    pub fn holds(&self, path: &std::path::Path) -> bool {
        self.0.contains_key(path)
    }

    /// The files changed in `later` that are not as they were in `self`,
    /// sorted.
    #[must_use]
    pub fn changed_since(&self, later: &Self) -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = later
            .0
            .iter()
            .filter(|(path, hash)| self.0.get(*path) != Some(hash))
            .map(|(path, _)| path.clone())
            .collect();
        paths.sort();
        paths
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::Fingerprint;

    #[test]
    fn only_what_changed_in_between_is_reported() {
        let dir = crate::tests::repository();
        let root = dir.path();
        fs::write(root.join("kept.txt"), "the user's edit\n").expect("write");
        let before = Fingerprint::of(&crate::status(root).expect("status"));
        fs::write(root.join("new.txt"), "the agent's\n").expect("write");
        let after = Fingerprint::of(&crate::status(root).expect("status"));
        assert_eq!(
            before.changed_since(&after),
            [std::path::PathBuf::from("new.txt")]
        );
        fs::write(root.join("kept.txt"), "the agent's too\n").expect("write");
        let last = Fingerprint::of(&crate::status(root).expect("status"));
        assert_eq!(before.changed_since(&last).len(), 2);
    }
}
