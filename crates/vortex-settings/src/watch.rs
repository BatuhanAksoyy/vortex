use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use anyhow::Context as _;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};

/// Calls `on_change` when any of the given files is created, modified or removed.
///
/// Parent directories are watched instead of the files themselves, because
/// editors often save by replacing the file and the files may not exist yet.
pub struct FileWatcher {
    _watcher: RecommendedWatcher,
}

impl FileWatcher {
    pub fn new(
        files: &[PathBuf],
        on_change: impl Fn() + Send + Sync + 'static,
    ) -> anyhow::Result<Self> {
        let targets: HashSet<PathBuf> = files.iter().cloned().collect();
        let mut watcher =
            notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
                if let Ok(event) = result
                    && is_relevant(&event, &targets)
                {
                    on_change();
                }
            })
            .context("cannot start file watcher")?;

        for dir in existing_parent_dirs(files) {
            watcher
                .watch(&dir, RecursiveMode::NonRecursive)
                .with_context(|| format!("cannot watch {}", dir.display()))?;
        }
        Ok(Self { _watcher: watcher })
    }
}

fn is_relevant(event: &notify::Event, targets: &HashSet<PathBuf>) -> bool {
    !matches!(event.kind, EventKind::Access(_)) && event.paths.iter().any(|p| targets.contains(p))
}

fn existing_parent_dirs(files: &[PathBuf]) -> HashSet<PathBuf> {
    files
        .iter()
        .filter_map(|file| file.parent())
        .filter(|dir| dir.is_dir())
        .map(Path::to_path_buf)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::FileWatcher;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn reports_changes_to_watched_file_only() {
        let dir = tempfile::tempdir().unwrap();
        let watched = dir.path().join("settings.json");
        let other = dir.path().join("other.json");
        let (sender, receiver) = mpsc::channel();
        let _watcher = FileWatcher::new(std::slice::from_ref(&watched), move || {
            let _ = sender.send(());
        })
        .unwrap();

        std::fs::write(&other, "{}").unwrap();
        assert!(receiver.recv_timeout(Duration::from_millis(300)).is_err());

        std::fs::write(&watched, "{}").unwrap();
        assert!(receiver.recv_timeout(Duration::from_secs(5)).is_ok());
    }
}
