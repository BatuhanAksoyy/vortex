//! The Claude and Codex subscription accounts Kiln runs sessions with.
//!
//! Every account shares its agent's usual directory, so sessions, settings
//! and skills are the same whichever account runs them, and a conversation
//! started with one account can be resumed with another. Only the sign-in
//! is kept apart: each added account has a credential store of its own
//! under Kiln's data directory. Until an account is added, sessions run
//! with the agent's usual sign-in.

mod codex_home;
mod usage;

use std::{
    collections::HashMap,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Context as _;
pub use codex_home::shared_home as codex_home;
use serde::{Deserialize, Serialize};
pub use usage::KnownUsage;

/// Moves only where the CLI keeps its sign-in (on macOS, which Keychain item
/// it uses). `CLAUDE_CONFIG_DIR` would move the session history too, so a
/// conversation could not follow the user to another account.
const CREDENTIALS_VAR: &str = "CLAUDE_SECURESTORAGE_CONFIG_DIR";

const LIST_FILE: &str = "accounts.json";
const USAGE_FILE: &str = "usage.json";
const CREDENTIALS_DIR: &str = "accounts";

/// An account added in Kiln, as its sign-in reported it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub email: String,
    #[serde(default)]
    pub plan: String,
    /// The harness the account signs in to, by its settings name; accounts
    /// kept before Codex came are Claude's.
    #[serde(default = "claude")]
    pub agent: String,
}

fn claude() -> String {
    CLAUDE.to_owned()
}

const CLAUDE: &str = "claude";
const CODEX: &str = "codex";

/// The added accounts, kept in `<dir>/accounts.json`, and the usage each
/// last reported, in `<dir>/usage.json`.
#[derive(Debug)]
pub struct Accounts {
    dir: PathBuf,
    added: Vec<Account>,
    usage: HashMap<String, KnownUsage>,
    /// When a chat last asked the CLI for an added account's usage.
    last_ask: Option<SystemTime>,
    /// The CLI's global configuration; `None` reads no snapshot.
    cli_config: Option<PathBuf>,
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
        let usage = usage::load(&dir.join(USAGE_FILE));
        Ok(Self {
            dir,
            added,
            usage,
            last_ask: None,
            cli_config: None,
        })
    }

    /// No added accounts, kept in `dir` once one is added.
    #[must_use]
    pub fn empty(dir: PathBuf) -> Self {
        Self {
            dir,
            added: Vec::new(),
            usage: HashMap::new(),
            last_ask: None,
            cli_config: None,
        }
    }

    /// Reads when the CLI last fetched usage before letting a chat ask for
    /// an added account's (`ask_usage`); without it, only Kiln's own asks
    /// are spaced out.
    #[must_use]
    pub fn reading_cli_usage(mut self) -> Self {
        self.cli_config = usage::cli_config();
        self
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

    /// The added accounts of harness `agent`, in the order they were added.
    pub fn added_for<'a>(&'a self, agent: &'a str) -> impl Iterator<Item = &'a Account> + 'a {
        self.added
            .iter()
            .filter(move |account| account.agent == agent)
    }

    /// The environment a session of account `id` runs with.
    #[must_use]
    pub fn env(&self, id: &str) -> Vec<(String, String)> {
        let agent = self
            .find(id)
            .map_or(CLAUDE, |account| account.agent.as_str());
        self.env_for(agent, id)
    }

    fn env_for(&self, agent: &str, id: &str) -> Vec<(String, String)> {
        let store = self.credentials(id);
        if agent == CODEX {
            return codex_home::env(&store);
        }
        vec![(
            CREDENTIALS_VAR.to_owned(),
            store.to_string_lossy().into_owned(),
        )]
    }

    /// A fresh id for an account of harness `agent` and the environment to
    /// sign it in with. Nothing is kept
    /// until `add` is called with the account the sign-in reported.
    ///
    /// Ids are never reused: on macOS a sign-in lives in a Keychain item named
    /// after its store's path, which outlives the store, so a reused id could
    /// come back signed in as a released account.
    pub fn reserve(&self, agent: &str) -> anyhow::Result<(String, Vec<(String, String)>)> {
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
        let env = self.env_for(agent, &id);
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

    /// The plan usage added account `id` last reported.
    #[must_use]
    pub fn usage(&self, id: &str) -> Option<KnownUsage> {
        self.usage.get(id).copied()
    }

    /// Whether a chat with added account `id` may ask the CLI for its usage
    /// at `now`. A yes counts as the ask. Asks are spaced out across all chats, as within a
    /// minute of a fetch the CLI answers with the numbers it fetched, which
    /// may be another account's; usage reported moments ago is not asked
    /// again.
    pub fn ask_usage(&mut self, id: &str, now: SystemTime) -> bool {
        let reported = self.usage(id).map(|known| known.reported_at);
        let fresh = reported
            .and_then(|at| now.duration_since(at).ok())
            .is_some_and(|age| age < usage::ASK_GAP);
        if fresh {
            return false;
        }
        let fetched_at = self.cli_config.as_deref().and_then(usage::cli_fetched_at);
        if !usage::may_ask(now, self.last_ask, fetched_at) {
            return false;
        }
        self.last_ask = Some(now);
        true
    }

    /// Keeps the usage added account `id` reported, across restarts.
    pub fn set_usage(&mut self, id: &str, known: KnownUsage) -> anyhow::Result<()> {
        if self.find(id).is_none() || self.usage(id) == Some(known) {
            return Ok(());
        }
        self.usage.insert(id.to_owned(), known);
        let text = serde_json::to_string_pretty(&self.usage)?;
        self.write(USAGE_FILE, &text)
    }

    /// The added account of harness `agent` signed in as `email`, if any.
    #[must_use]
    pub fn find_email(&self, agent: &str, email: &str) -> Option<&Account> {
        self.added
            .iter()
            .find(|account| account.agent == agent && account.email.eq_ignore_ascii_case(email))
    }

    fn credentials(&self, id: &str) -> PathBuf {
        self.dir.join(CREDENTIALS_DIR).join(id)
    }

    fn save(&self) -> anyhow::Result<()> {
        let text = serde_json::to_string_pretty(&self.added)?;
        self.write(LIST_FILE, &text)
    }

    /// Writes a temporary file and renames it over `name`, so a crash
    /// mid-write leaves the previous file intact.
    fn write(&self, name: &str, text: &str) -> anyhow::Result<()> {
        std::fs::create_dir_all(&self.dir)
            .with_context(|| format!("cannot create {}", self.dir.display()))?;
        let path = self.dir.join(name);
        let temporary = path.with_extension("json.tmp");
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
mod tests;
