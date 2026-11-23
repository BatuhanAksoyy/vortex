//! The Claude subscription accounts Kiln runs sessions with.
//!
//! Every account shares the user's Claude Code directory, so sessions,
//! settings and skills are the same whichever account runs them, and a
//! conversation started with one account can be resumed with another. Only
//! the sign-in is kept apart: each added account has a credential store of
//! its own under Kiln's data directory. Until an account is added, sessions
//! run with the user's usual Claude Code sign-in.

use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Context as _;
use serde::{Deserialize, Serialize};

/// Moves only where the CLI keeps its sign-in (on macOS, which Keychain item
/// it uses). `CLAUDE_CONFIG_DIR` would move the session history too, so a
/// conversation could not follow the user to another account.
const CREDENTIALS_VAR: &str = "CLAUDE_SECURESTORAGE_CONFIG_DIR";

const LIST_FILE: &str = "accounts.json";
const CREDENTIALS_DIR: &str = "accounts";

/// An account added in Kiln, as its sign-in reported it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub email: String,
    #[serde(default)]
    pub plan: String,
}

/// The added accounts, kept in `<dir>/accounts.json`.
#[derive(Debug)]
pub struct Accounts {
    dir: PathBuf,
    added: Vec<Account>,
}

impl Accounts {
    /// Reads the list kept in `dir`; a missing list is an empty one.
    pub fn load(dir: PathBuf) -> anyhow::Result<Self> {
        let path = dir.join(LIST_FILE);
        let added = match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text)
                .with_context(|| format!("cannot read {}", path.display()))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => {
                return Err(error).with_context(|| format!("cannot read {}", path.display()));
            }
        };
        Ok(Self { dir, added })
    }

    /// No added accounts, kept in `dir` once one is added.
    #[must_use]
    pub fn empty(dir: PathBuf) -> Self {
        Self {
            dir,
            added: Vec::new(),
        }
    }

    /// The accounts added in Kiln, in the order they were added.
    #[must_use]
    pub fn added(&self) -> &[Account] {
        &self.added
    }

    #[must_use]
    pub fn find(&self, id: &str) -> Option<&Account> {
        self.added.iter().find(|account| account.id == id)
    }

    /// The environment a session of account `id` runs with.
    #[must_use]
    pub fn env(&self, id: &str) -> Vec<(String, String)> {
        let store = self.credentials(id);
        vec![(
            CREDENTIALS_VAR.to_owned(),
            store.to_string_lossy().into_owned(),
        )]
    }

    /// A fresh id and the environment to sign it in with. Nothing is kept
    /// until `add` is called with the account the sign-in reported.
    ///
    /// Ids are never reused: on macOS a sign-in lives in a Keychain item named
    /// after its store's path, which outlives the store, so a reused id could
    /// come back signed in as a released account.
    pub fn reserve(&self) -> anyhow::Result<(String, Vec<(String, String)>)> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let id = (now..=u128::MAX)
            .map(|stamp| format!("account-{stamp}"))
            .find(|id| !self.credentials(id).exists())
            .unwrap_or_default();
        let store = self.credentials(&id);
        std::fs::create_dir_all(&store)
            .with_context(|| format!("cannot create {}", store.display()))?;
        let env = self.env(&id);
        Ok((id, env))
    }

    /// Keeps `account`, signed in under an id from `reserve`.
    pub fn add(&mut self, account: Account) -> anyhow::Result<()> {
        self.added.push(account);
        self.save()
    }

    /// Forgets a reserved id whose sign-in did not finish, or that signed in
    /// to an account already added.
    pub fn release(&self, id: &str) {
        if self.find(id).is_none() {
            let _ = std::fs::remove_dir_all(self.credentials(id));
        }
    }

    /// The added account signed in as `email`, if any.
    #[must_use]
    pub fn find_email(&self, email: &str) -> Option<&Account> {
        self.added
            .iter()
            .find(|account| account.email.eq_ignore_ascii_case(email))
    }

    fn credentials(&self, id: &str) -> PathBuf {
        self.dir.join(CREDENTIALS_DIR).join(id)
    }

    /// Writes a temporary file and renames it over the list, so a crash
    /// mid-write leaves the previous list intact.
    fn save(&self) -> anyhow::Result<()> {
        std::fs::create_dir_all(&self.dir)
            .with_context(|| format!("cannot create {}", self.dir.display()))?;
        let path = self.dir.join(LIST_FILE);
        let temporary = path.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(&self.added)?;
        std::fs::write(&temporary, text)
            .with_context(|| format!("cannot write {}", temporary.display()))?;
        std::fs::rename(&temporary, &path)
            .with_context(|| format!("cannot write {}", path.display()))
    }
}

/// Kiln's data directory: `~/Library/Application Support/kiln` on macOS,
/// `~/.local/share/kiln` on Linux. Not under a temporary directory, which the
/// CLIs may refuse to keep state in.
#[must_use]
pub fn data_dir() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|dirs| dirs.data_dir().join("kiln"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(id: &str, email: &str) -> Account {
        Account {
            id: id.into(),
            email: email.into(),
            plan: "Claude Pro".into(),
        }
    }

    #[test]
    fn an_added_account_has_a_credential_store_of_its_own() {
        let dir = tempfile::tempdir().unwrap();
        let mut accounts = Accounts::load(dir.path().to_owned()).unwrap();
        let (id, env) = accounts.reserve().unwrap();
        assert!(id.starts_with("account-"), "{id}");
        let store = dir.path().join("accounts").join(&id);
        assert_eq!(
            env,
            [(
                CREDENTIALS_VAR.to_owned(),
                store.to_string_lossy().into_owned()
            )]
        );
        assert!(store.is_dir());
        accounts.add(account(&id, "a@example.com")).unwrap();
        assert_ne!(accounts.reserve().unwrap().0, id);
    }

    #[test]
    fn added_accounts_are_kept_across_loads() {
        let dir = tempfile::tempdir().unwrap();
        let mut accounts = Accounts::load(dir.path().to_owned()).unwrap();
        let (id, _) = accounts.reserve().unwrap();
        accounts.add(account(&id, "a@example.com")).unwrap();
        let again = Accounts::load(dir.path().to_owned()).unwrap();
        assert_eq!(again.added(), [account(&id, "a@example.com")]);
        assert_eq!(again.find(&id), Some(&account(&id, "a@example.com")));
        assert_eq!(
            again.find_email("A@example.com").map(|a| a.id.as_str()),
            Some(id.as_str())
        );
    }

    #[test]
    fn an_unfinished_sign_in_leaves_nothing_behind() {
        let dir = tempfile::tempdir().unwrap();
        let accounts = Accounts::load(dir.path().to_owned()).unwrap();
        let (id, _) = accounts.reserve().unwrap();
        accounts.release(&id);
        assert!(!dir.path().join("accounts").join(&id).exists());
        assert_eq!(accounts.added(), Vec::new());
    }

    #[test]
    fn a_damaged_list_is_an_error_not_an_empty_list() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(LIST_FILE), "{").unwrap();
        assert!(Accounts::load(dir.path().to_owned()).is_err());
    }
}
