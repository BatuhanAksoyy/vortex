//! A Codex account's own `CODEX_HOME`, which holds its sign-in. The user's
//! Codex configuration and stored conversations are linked in from their
//! usual home, and its thread index is the usual one, so they are the same
//! whichever account runs a chat and a thread can follow the user to
//! another account.

use std::path::{Path, PathBuf};

pub const HOME_VAR: &str = "CODEX_HOME";
/// Where Codex keeps its databases, among them the index of stored threads.
const SQLITE_VAR: &str = "CODEX_SQLITE_HOME";

/// What is shared from the usual home; everything else, the sign-in among
/// it, stays the account's own.
const SHARED: &[&str] = &[
    "config.toml",
    "AGENTS.md",
    "sessions",
    "archived_sessions",
    "skills",
    "rules",
    "prompts",
];

/// The usual home: `CODEX_HOME` as Vortex was started with it, else `~/.codex`.
#[must_use]
pub fn shared_home() -> Option<PathBuf> {
    std::env::var_os(HOME_VAR)
        .map(PathBuf::from)
        .or_else(|| directories::BaseDirs::new().map(|dirs| dirs.home_dir().join(".codex")))
}

/// The environment of a session of the account whose home is `home`. Links
/// in whatever the usual home has gained since.
pub fn env(home: &Path) -> Vec<(String, String)> {
    let mut env = vec![(HOME_VAR.to_owned(), home.to_string_lossy().into_owned())];
    if let Some(shared) = shared_home() {
        link_shared(home, &shared);
        env.push((SQLITE_VAR.to_owned(), shared.to_string_lossy().into_owned()));
    }
    env
}

fn link_shared(home: &Path, shared: &Path) {
    // Stored threads must land in the shared folder from the first one on.
    let _ = std::fs::create_dir_all(shared.join("sessions"));
    for name in SHARED {
        let (from, to) = (shared.join(name), home.join(name));
        if from.exists() && std::fs::symlink_metadata(&to).is_err() {
            link(&from, &to);
        }
    }
}

#[cfg(unix)]
fn link(from: &Path, to: &Path) {
    if let Err(error) = std::os::unix::fs::symlink(from, to) {
        log::warn!(
            "cannot link {} into a Codex account: {error}",
            from.display()
        );
    }
}

/// Windows needs a privilege for symbolic links; there the account keeps
/// its own configuration until Vortex supports it.
#[cfg(not(unix))]
fn link(_from: &Path, _to: &Path) {}

#[cfg(all(test, unix))]
mod tests {
    use super::link_shared;

    #[test]
    fn the_configuration_and_threads_are_linked_but_not_the_sign_in() {
        let dir = tempfile::tempdir().unwrap();
        let (shared, home) = (dir.path().join("shared"), dir.path().join("home"));
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&shared).unwrap();
        std::fs::write(shared.join("config.toml"), "model = \"x\"\n").unwrap();
        std::fs::write(shared.join("auth.json"), "{}").unwrap();
        link_shared(&home, &shared);
        assert_eq!(
            std::fs::read_to_string(home.join("config.toml")).unwrap(),
            "model = \"x\"\n"
        );
        assert!(
            home.join("sessions").is_dir(),
            "made in the shared home and linked"
        );
        assert!(!home.join("auth.json").exists());
        link_shared(&home, &shared);
    }
}
