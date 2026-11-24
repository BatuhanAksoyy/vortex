//! Files a page sends to be kept.

use std::path::{Path, PathBuf};

use anyhow::Context as _;
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadState {
    Running,
    /// Received, and to be put in its place.
    Received,
    Canceled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadProgress {
    pub id: String,
    pub received: u64,
    /// Zero when the site does not say.
    pub total: u64,
    pub state: DownloadState,
}

pub(crate) fn progress(params: &Value) -> Option<DownloadProgress> {
    // Byte counts are sent as floating-point numbers.
    let bytes = |key: &str| {
        params[key]
            .as_number()?
            .to_string()
            .split('.')
            .next()?
            .parse()
            .ok()
    };
    Some(DownloadProgress {
        id: params["guid"].as_str()?.to_owned(),
        received: bytes("receivedBytes").unwrap_or(0),
        total: bytes("totalBytes").unwrap_or(0),
        state: match params["state"].as_str()? {
            "completed" => DownloadState::Received,
            "canceled" => DownloadState::Canceled,
            _ => DownloadState::Running,
        },
    })
}

/// The user's downloads folder.
#[must_use]
pub fn downloads_dir() -> Option<PathBuf> {
    let user = directories::UserDirs::new()?;
    user.download_dir().map(Path::to_path_buf)
}

/// Moves `received` into `folder` under a name like `name`, beside files that
/// have it already.
pub(crate) fn keep(received: &Path, folder: &Path, name: &str) -> anyhow::Result<PathBuf> {
    std::fs::create_dir_all(folder)
        .with_context(|| format!("cannot create {}", folder.display()))?;
    let name = file_name(name);
    for attempt in 0..1000 {
        let path = folder.join(numbered(&name, attempt));
        // Reserved first, so nothing is written over.
        match std::fs::File::create_new(&path) {
            Ok(_) => {
                let moved = move_file(received, &path);
                return moved
                    .with_context(|| format!("cannot write {}", path.display()))
                    .map(|()| path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(error).with_context(|| format!("cannot write {}", path.display()));
            }
        }
    }
    anyhow::bail!("{name} is there too many times already")
}

fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    // On another disk.
    std::fs::copy(from, to)?;
    std::fs::remove_file(from)
}

/// Only a name: a site must not choose the folder, or hide the file.
fn file_name(suggested: &str) -> String {
    let last = suggested.rsplit(['/', '\\']).next().unwrap_or_default();
    let clean: String = last.chars().filter(|c| !c.is_control()).collect();
    let name = clean.trim().trim_start_matches('.');
    if name.is_empty() {
        return "download".to_owned();
    }
    name.to_owned()
}

fn numbered(name: &str, attempt: usize) -> String {
    if attempt == 0 {
        return name.to_owned();
    }
    match name.rsplit_once('.') {
        Some((stem, extension)) => format!("{stem} ({attempt}).{extension}"),
        None => format!("{name} ({attempt})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn nothing_is_written_over_or_outside() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("Downloads");
        let receive = |name: &str| {
            let received = dir.path().join("guid");
            std::fs::write(&received, name).unwrap();
            keep(&received, &folder, name).unwrap()
        };
        assert_eq!(receive("a.txt"), folder.join("a.txt"));
        assert_eq!(receive("a.txt"), folder.join("a (1).txt"));
        assert_eq!(receive("../../a.txt"), folder.join("a (2).txt"));
        assert_eq!(receive(".zshrc"), folder.join("zshrc"));
        assert_eq!(receive(".."), folder.join("download"));
    }

    #[test]
    fn progress_is_read_from_whole_and_fractional_numbers() {
        let params =
            json!({"guid": "g", "totalBytes": 10.0, "receivedBytes": 4, "state": "inProgress"});
        let progress = progress(&params).unwrap();
        assert_eq!((progress.received, progress.total), (4, 10));
        assert_eq!(progress.state, DownloadState::Running);
    }
}
