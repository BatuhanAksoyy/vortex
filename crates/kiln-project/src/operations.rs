//! Creating, renaming and deleting a project's files.

use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context as _, bail, ensure};

/// Creates the empty file `name` in `folder`. A name with folders in it,
/// such as `ui/list.rs`, creates those too.
///
/// # Errors
/// When the name is not usable or taken, or the disk refuses.
pub fn create_file(folder: &Path, name: &str) -> anyhow::Result<PathBuf> {
    let path = new_path(folder, name)?;
    create_parent(&path)?;
    // Not written over a file that appeared since the check.
    fs::File::create_new(&path).with_context(|| format!("cannot create {name}"))?;
    Ok(path)
}

/// Creates the folder `name` in `folder`, with the folders above it.
///
/// # Errors
/// When the name is not usable or taken, or the disk refuses.
pub fn create_folder(folder: &Path, name: &str) -> anyhow::Result<PathBuf> {
    let path = new_path(folder, name)?;
    fs::create_dir_all(&path).with_context(|| format!("cannot create {name}"))?;
    Ok(path)
}

/// Renames the file or folder at `path` to `name`, moving it when the name
/// has folders in it.
///
/// # Errors
/// When the name is not usable or taken, or the disk refuses.
pub fn rename(path: &Path, name: &str) -> anyhow::Result<PathBuf> {
    let folder = path.parent().context("it has no folder")?;
    let to = renamed_path(path, folder, name)?;
    ensure!(
        !to.starts_with(path) || to == path,
        "it cannot go into itself"
    );
    create_parent(&to)?;
    fs::rename(path, &to).with_context(|| format!("cannot rename to {name}"))?;
    Ok(to)
}

/// Moves the file or folder at `path` to the system's trash.
///
/// # Errors
/// When the system refuses.
pub fn trash(path: &Path) -> anyhow::Result<()> {
    trash_context()
        .delete(path)
        .with_context(|| format!("cannot move {} to the trash", path.display()))
}

#[cfg(target_os = "macos")]
fn trash_context() -> trash::TrashContext {
    use trash::macos::{DeleteMethod, TrashContextExtMacos as _};
    let mut context = trash::TrashContext::new();
    // The default drives Finder, which asks the user to allow it.
    context.set_delete_method(DeleteMethod::NsFileManager);
    context
}

#[cfg(not(target_os = "macos"))]
fn trash_context() -> trash::TrashContext {
    trash::TrashContext::new()
}

fn create_parent(path: &Path) -> anyhow::Result<()> {
    let parent = path.parent().context("it has no folder")?;
    fs::create_dir_all(parent).with_context(|| format!("cannot create {}", parent.display()))
}

fn renamed_path(path: &Path, folder: &Path, name: &str) -> anyhow::Result<PathBuf> {
    let to = folder.join(name);
    // On a file system that ignores case the new name finds the old file.
    let only_case = path.to_string_lossy().to_lowercase() == to.to_string_lossy().to_lowercase();
    if only_case {
        return Ok(to);
    }
    new_path(folder, name)
}

fn new_path(folder: &Path, name: &str) -> anyhow::Result<PathBuf> {
    ensure!(!name.trim().is_empty(), "the name is empty");
    let inside = Path::new(name)
        .components()
        .all(|part| matches!(part, Component::Normal(_)));
    if !inside {
        bail!("“{name}” is not a name inside the folder");
    }
    let path = folder.join(name);
    ensure!(
        fs::symlink_metadata(&path).is_err(),
        "“{name}” already exists"
    );
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_and_folders_are_created_with_the_folders_above() {
        let dir = tempfile::tempdir().unwrap();
        let file = create_file(dir.path(), "ui/list.rs").unwrap();
        assert!(file.is_file());
        assert!(create_folder(dir.path(), "a/b").unwrap().is_dir());
        assert!(create_file(dir.path(), "ui/list.rs").is_err(), "taken");
        assert!(create_folder(dir.path(), "ui").is_err(), "taken");
    }

    #[test]
    fn names_stay_inside_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["", "  ", "../up", "/root", "a/../../b"] {
            assert!(create_file(dir.path(), name).is_err(), "{name}");
        }
    }

    #[test]
    fn renaming_never_replaces_another_file() {
        let dir = tempfile::tempdir().unwrap();
        let one = create_file(dir.path(), "one").unwrap();
        fs::write(&one, "kept").unwrap();
        create_file(dir.path(), "two").unwrap();
        assert!(rename(&one, "two").is_err());
        let upper = rename(&one, "ONE").unwrap();
        let moved = rename(&upper, "in/three").unwrap();
        assert_eq!(fs::read_to_string(moved).unwrap(), "kept");
        let folder = dir.path().join("in");
        assert!(rename(&folder, "in/deeper").is_err(), "into itself");
    }
}
