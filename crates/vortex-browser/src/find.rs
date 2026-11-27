//! The Chromium-based browsers on this machine.

use std::path::PathBuf;

/// A browser to run, by the name the user knows it by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installed {
    pub name: &'static str,
    pub path: PathBuf,
}

/// In the order they are tried.
#[cfg(target_os = "macos")]
const CANDIDATES: [(&str, &str); 5] = [
    (
        "Google Chrome",
        "Google Chrome.app/Contents/MacOS/Google Chrome",
    ),
    (
        "Microsoft Edge",
        "Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
    ),
    ("Brave", "Brave Browser.app/Contents/MacOS/Brave Browser"),
    ("Chromium", "Chromium.app/Contents/MacOS/Chromium"),
    ("Vivaldi", "Vivaldi.app/Contents/MacOS/Vivaldi"),
];
#[cfg(not(target_os = "macos"))]
const CANDIDATES: [(&str, &str); 7] = [
    ("Google Chrome", "google-chrome"),
    ("Google Chrome", "google-chrome-stable"),
    ("Microsoft Edge", "microsoft-edge"),
    ("Brave", "brave-browser"),
    ("Chromium", "chromium"),
    ("Chromium", "chromium-browser"),
    ("Vivaldi", "vivaldi"),
];

/// The first installed browser Vortex can draw pages with.
#[must_use]
pub fn find() -> Option<Installed> {
    let folders = folders();
    CANDIDATES.into_iter().find_map(|(name, file)| {
        let mut paths = folders.iter().map(|folder| folder.join(file));
        let path = paths.find(|path| path.is_file())?;
        Some(Installed { name, path })
    })
}

#[cfg(target_os = "macos")]
fn folders() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Applications"));
    std::iter::once(PathBuf::from("/Applications"))
        .chain(home)
        .collect()
}

#[cfg(not(target_os = "macos"))]
fn folders() -> Vec<PathBuf> {
    std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default()
}
