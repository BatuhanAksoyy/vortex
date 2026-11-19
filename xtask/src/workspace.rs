use std::path::{Path, PathBuf};

use anyhow::Context as _;

/// The repository root: the parent of this crate's manifest directory.
pub fn root() -> anyhow::Result<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .context("xtask must live one level below the repository root")
}

/// Every `.rs` file in the repository, honoring `.gitignore`.
pub fn rust_files(root: &Path) -> Vec<PathBuf> {
    files_with_extension(root, "rs")
}

pub fn files_with_extension(root: &Path, extension: &str) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = ignore::WalkBuilder::new(root)
        .hidden(false)
        .filter_entry(|entry| entry.file_name() != ".git")
        .build()
        .flatten()
        .filter(|entry| entry.file_type().is_some_and(|t| t.is_file()))
        .map(ignore::DirEntry::into_path)
        .filter(|path| path.extension().is_some_and(|ext| ext == extension))
        .collect();
    files.sort();
    files
}

pub fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
