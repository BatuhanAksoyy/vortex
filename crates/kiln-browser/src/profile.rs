//! Where the browser keeps cookies and the like. One browser may use it at a
//! time, so each running Kiln has its own.

use std::{
    fs::{File, TryLockError},
    path::{Path, PathBuf},
};

use anyhow::{Context as _, bail};

/// As many Kilns as may run.
const MOST: usize = 20;

pub struct Profile {
    path: PathBuf,
    first: bool,
    /// Holds the claim; the system lets go of it with the process.
    _lock: File,
}

impl Profile {
    /// Claims the profile at `path`, or the first free one beside it.
    pub fn claim(path: &Path) -> anyhow::Result<Self> {
        for number in 1..=MOST {
            let path = numbered(path, number);
            std::fs::create_dir_all(&path)
                .with_context(|| format!("cannot create {}", path.display()))?;
            let lock = File::create(path.join("kiln.lock"))
                .with_context(|| format!("cannot write in {}", path.display()))?;
            match lock.try_lock() {
                Ok(()) => {
                    return Ok(Self {
                        path,
                        first: number == 1,
                        _lock: lock,
                    });
                }
                Err(TryLockError::WouldBlock) => {}
                Err(TryLockError::Error(error)) => {
                    return Err(error).with_context(|| format!("cannot lock {}", path.display()));
                }
            }
        }
        bail!("Too many Kilns are running.")
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Whether this is the profile used when no other Kiln runs, with the
    /// sign-ins the user expects.
    #[must_use]
    pub fn is_first(&self) -> bool {
        self.first
    }
}

fn numbered(path: &Path, number: usize) -> PathBuf {
    if number == 1 {
        return path.to_path_buf();
    }
    let mut name = path.file_name().unwrap_or_default().to_owned();
    name.push(format!("-{number}"));
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_claim_gets_its_own_until_the_first_ends() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("browser");
        let first = Profile::claim(&path).unwrap();
        let second = Profile::claim(&path).unwrap();
        assert!(first.is_first() && !second.is_first());
        assert_eq!(second.path(), dir.path().join("browser-2"));
        drop(first);
        // A browser another test starts holds a copy of the lock between
        // fork and exec, which keeps it a moment longer.
        let again = (0..100).find_map(|_| {
            let claim = Profile::claim(&path).unwrap();
            if claim.is_first() {
                return Some(claim);
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
            None
        });
        assert!(again.is_some());
    }
}
