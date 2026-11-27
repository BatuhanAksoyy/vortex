use std::path::{Path, PathBuf};

use anyhow::Context as _;
use ignore::{WalkBuilder, overrides::OverrideBuilder};

#[derive(Debug, Clone)]
pub struct ListOptions {
    /// Glob patterns hidden in addition to `.gitignore` rules.
    pub exclude: Vec<String>,
    /// Upper bound on returned entries, so a huge tree cannot stall the UI.
    pub limit: usize,
}

/// Project-relative paths, sorted, with directories marked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileList {
    pub entries: Vec<FileEntry>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub path: PathBuf,
    pub is_dir: bool,
}

impl FileList {
    pub fn files(&self) -> impl Iterator<Item = &Path> {
        self.entries
            .iter()
            .filter(|e| !e.is_dir)
            .map(|e| e.path.as_path())
    }
}

/// Lists the project honoring `.gitignore`, `.ignore` and global git excludes.
/// Dotfiles are shown (as in VSCode); `options.exclude` hides more.
pub fn list_files(root: &Path, options: &ListOptions) -> anyhow::Result<FileList> {
    let mut overrides = OverrideBuilder::new(root);
    for pattern in &options.exclude {
        overrides
            .add(&format!("!{pattern}"))
            .with_context(|| format!("invalid exclude pattern `{pattern}`"))?;
    }
    let walker = WalkBuilder::new(root)
        .hidden(false)
        .require_git(false)
        .overrides(overrides.build().context("invalid exclude patterns")?)
        .build();

    let mut list = FileList::default();
    for entry in walker.flatten() {
        let Ok(relative) = entry.path().strip_prefix(root) else {
            continue;
        };
        if relative.as_os_str().is_empty() {
            continue;
        }
        if list.entries.len() >= options.limit {
            list.truncated = true;
            break;
        }
        list.entries.push(FileEntry {
            path: relative.to_path_buf(),
            is_dir: entry.file_type().is_some_and(|t| t.is_dir()),
        });
    }
    list.entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::{ListOptions, list_files};
    use std::path::{Path, PathBuf};

    fn touch(root: &Path, relative: &str) {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "").unwrap();
    }

    fn options(exclude: &[&str], limit: usize) -> ListOptions {
        ListOptions {
            exclude: exclude.iter().map(|s| (*s).to_owned()).collect(),
            limit,
        }
    }

    #[test]
    fn honors_gitignore_and_excludes_but_shows_dotfiles() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        touch(root, ".gitignore");
        std::fs::write(root.join(".gitignore"), "target/\n").unwrap();
        touch(root, "src/main.rs");
        touch(root, "target/debug/app");
        touch(root, ".env.example");
        touch(root, "notes.tmp");

        let list = list_files(root, &options(&["*.tmp"], 100)).unwrap();
        let files: Vec<PathBuf> = list.files().map(Path::to_path_buf).collect();
        assert_eq!(
            files,
            [".env.example", ".gitignore", "src/main.rs"].map(PathBuf::from)
        );
        assert!(!list.truncated);
    }

    #[test]
    fn stops_at_limit() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["a", "b", "c"] {
            touch(dir.path(), name);
        }
        let list = list_files(dir.path(), &options(&[], 2)).unwrap();
        assert_eq!(list.entries.len(), 2);
        assert!(list.truncated);
    }

    #[test]
    fn rejects_invalid_exclude_glob() {
        let dir = tempfile::tempdir().unwrap();
        assert!(list_files(dir.path(), &options(&["a[b"], 10)).is_err());
    }
}
