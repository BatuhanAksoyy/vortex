//! Where an agent's CLI is: the path the user chose, else its name on
//! `PATH`, else where its installers put it. An app started from the Dock
//! may have a `PATH` without it, and an older install may sit only behind
//! a shell alias.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, PoisonError, RwLock},
};

use crate::install::locate;

/// One CLI's program, shared by everything that runs it, so a path the
/// user chooses applies to the next start of any of them.
#[derive(Clone, Debug)]
pub struct Program {
    /// A bare name to look for, or a path.
    name: PathBuf,
    /// Under the home folder, where its installers put it.
    known: &'static [&'static str],
    chosen: Arc<RwLock<Option<PathBuf>>>,
}

/// Where installers put programs whatever they are: Homebrew on either
/// chip, and `MacPorts`.
const SHARED: [&str; 3] = ["/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin"];

/// Under the home folder, where Node's package managers and version
/// managers keep global installs, which both CLIs can be.
const NODE_BINS: [&str; 6] = [
    ".volta/bin",
    "Library/pnpm",
    ".local/share/pnpm",
    ".yarn/bin",
    ".asdf/shims",
    ".local/share/mise/shims",
];

/// Under the home folder, a folder per Node version and where in each the
/// global installs are: nvm's and fnm's.
const NODE_VERSIONS: [(&str, &str); 3] = [
    (".nvm/versions/node", "bin"),
    (".local/share/fnm/node-versions", "installation/bin"),
    (
        "Library/Application Support/fnm/node-versions",
        "installation/bin",
    ),
];

impl Program {
    /// `name`, found also at `known`, each a path under the home folder.
    #[must_use]
    pub fn new(name: impl Into<PathBuf>, known: &'static [&'static str]) -> Self {
        Self {
            name: name.into(),
            known,
            chosen: Arc::default(),
        }
    }

    /// The path to run: the chosen one, else the first found, else the bare
    /// name, which then fails to start as a missing program does.
    #[must_use]
    pub fn path(&self) -> PathBuf {
        if let Some(chosen) = self.chosen() {
            return chosen;
        }
        let home = std::env::var_os("HOME").map(PathBuf::from);
        locate(&self.name)
            .or_else(|| found(&self.name, self.known, home.as_deref()))
            .unwrap_or_else(|| self.name.clone())
    }

    /// Runs `path` from now on; `None` goes back to looking for it.
    /// Returns whether that changed anything.
    pub fn choose(&self, path: Option<PathBuf>) -> bool {
        let mut chosen = self.chosen.write().unwrap_or_else(PoisonError::into_inner);
        let changed = *chosen != path;
        *chosen = path;
        changed
    }

    fn chosen(&self) -> Option<PathBuf> {
        let chosen = self.chosen.read().unwrap_or_else(PoisonError::into_inner);
        chosen.clone()
    }
}

impl From<&str> for Program {
    fn from(name: &str) -> Self {
        Self::new(name, &[])
    }
}

impl From<PathBuf> for Program {
    fn from(path: PathBuf) -> Self {
        Self::new(path, &[])
    }
}

/// The first of `known` under `home`, then of the shared folders, then of
/// Node's folders, that is a file.
fn found(name: &Path, known: &[&str], home: Option<&Path>) -> Option<PathBuf> {
    let own = home
        .into_iter()
        .flat_map(|home| known.iter().map(move |path| home.join(path)));
    let shared = SHARED.iter().map(PathBuf::from);
    let node = home.into_iter().flat_map(node_dirs);
    own.chain(shared.chain(node).map(|dir| dir.join(name)))
        .find(|candidate| candidate.is_file())
        .and_then(|candidate| std::fs::canonicalize(candidate).ok())
}

/// Node's folders of global installs under `home`, the newest version's
/// first where there is one per version.
fn node_dirs(home: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = NODE_BINS.iter().map(|dir| home.join(dir)).collect();
    for (versions, bin) in NODE_VERSIONS {
        let Ok(entries) = std::fs::read_dir(home.join(versions)) else {
            continue;
        };
        let mut each: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
        each.sort_by_key(|dir| std::cmp::Reverse(version_of(dir)));
        dirs.extend(each.into_iter().map(|dir| dir.join(bin)));
    }
    dirs
}

/// `v20.11.0` as numbers, so `v20` sorts after `v9`.
fn version_of(dir: &Path) -> Vec<u32> {
    let name = dir.file_name().and_then(|name| name.to_str()).unwrap_or("");
    let name = name.trim_start_matches('v');
    name.split('.')
        .map(|part| part.parse().unwrap_or(0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chosen_path_is_run_and_choosing_none_looks_again() {
        let program = Program::new("vortex-no-such-cli", &[]);
        assert_eq!(program.path(), PathBuf::from("vortex-no-such-cli"));
        assert!(program.choose(Some("/opt/cli".into())));
        assert!(!program.choose(Some("/opt/cli".into())), "no change");
        assert_eq!(program.clone().path(), PathBuf::from("/opt/cli"), "shared");
        assert!(program.choose(None));
        assert_eq!(program.path(), PathBuf::from("vortex-no-such-cli"));
    }

    #[test]
    fn the_newest_node_version_s_install_is_found() {
        let home = tempfile::tempdir().unwrap();
        for version in ["v9.0.0", "v20.11.0"] {
            let bin = home
                .path()
                .join(".nvm/versions/node")
                .join(version)
                .join("bin");
            std::fs::create_dir_all(&bin).unwrap();
            std::fs::write(bin.join("vortex-no-such-cli"), "").unwrap();
        }
        let found = found(Path::new("vortex-no-such-cli"), &[], Some(home.path()));
        let newest = home
            .path()
            .join(".nvm/versions/node/v20.11.0/bin/vortex-no-such-cli");
        assert_eq!(found, newest.canonicalize().ok());
    }

    #[test]
    fn an_install_off_path_is_found_where_its_installer_puts_it() {
        let home = tempfile::tempdir().unwrap();
        let local = home.path().join(".claude/local");
        std::fs::create_dir_all(&local).unwrap();
        std::fs::write(local.join("claude"), "").unwrap();
        let known = [".local/bin/claude", ".claude/local/claude"];
        let found = found(Path::new("vortex-no-such-cli"), &known, Some(home.path()));
        assert_eq!(
            found.and_then(|path| path.canonicalize().ok()),
            local.join("claude").canonicalize().ok()
        );
    }
}
