//! The plan usage each added account last reported. Only a reply carries an
//! added account's usage, so without it a chat would show none after a
//! restart until the account's first reply.

use std::{
    collections::HashMap,
    path::Path,
    time::{Duration, SystemTime},
};

use kiln_agent::Usage;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnownUsage {
    pub usage: Usage,
    pub reported_at: SystemTime,
}

/// The usage kept at `path`, by account id. A missing or damaged file is
/// empty: the numbers only fill the meter until the next reply.
pub(crate) fn load(path: &Path) -> HashMap<String, KnownUsage> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// How long the CLI answers `get_usage` from the snapshot any account last
/// fetched, which may be another account's (CLI 2.1.326). Past it, the CLI
/// asks with its own account's sign-in.
const SNAPSHOT_TRUSTED: Duration = Duration::from_secs(60);
/// Kept between asks, so each finds the snapshot too old to answer from.
pub(crate) const ASK_GAP: Duration = Duration::from_secs(70);

/// The CLI's global configuration, where it keeps its usage snapshot.
pub(crate) fn cli_config() -> Option<std::path::PathBuf> {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(|dir| Path::new(&dir).join(".claude.json"))
        .or_else(|| std::env::home_dir().map(|home| home.join(".claude.json")))
}

/// When the CLI last fetched plan usage for any account, as the snapshot in
/// `config` says.
pub(crate) fn cli_fetched_at(config: &Path) -> Option<SystemTime> {
    snapshot_fetched_at(&std::fs::read_to_string(config).ok()?)
}

fn snapshot_fetched_at(config: &str) -> Option<SystemTime> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Config {
        cached_usage_utilization: Option<Snapshot>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Snapshot {
        fetched_at_ms: u64,
    }
    let snapshot = serde_json::from_str::<Config>(config)
        .ok()?
        .cached_usage_utilization?;
    SystemTime::UNIX_EPOCH.checked_add(Duration::from_millis(snapshot.fetched_at_ms))
}

/// Whether asking at `now` gets a fresh answer: no snapshot the CLI would
/// answer from instead, and no ask of Kiln's own that leaves one.
pub(crate) fn may_ask(
    now: SystemTime,
    last_ask: Option<SystemTime>,
    fetched_at: Option<SystemTime>,
) -> bool {
    let older = |at: Option<SystemTime>, than: Duration| {
        at.is_none_or(|at| now.duration_since(at).is_ok_and(|age| age >= than))
    };
    older(last_ask, ASK_GAP) && older(fetched_at, SNAPSHOT_TRUSTED + Duration::from_secs(5))
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use super::{may_ask, snapshot_fetched_at};

    #[test]
    fn reads_when_the_cli_last_fetched_usage() {
        let config = r#"{"other":1,"cachedUsageUtilization":{"fetchedAtMs":1000,"accountUuid":"a","utilization":{}}}"#;
        let at = SystemTime::UNIX_EPOCH + Duration::from_secs(1);
        assert_eq!(snapshot_fetched_at(config), Some(at));
        assert_eq!(snapshot_fetched_at(r#"{"other":1}"#), None);
        assert_eq!(snapshot_fetched_at("not json"), None);
    }

    #[test]
    fn asks_only_past_the_snapshot_and_its_own_last_ask() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
        let ago = |secs| Some(now - Duration::from_secs(secs));
        assert!(may_ask(now, None, None));
        assert!(may_ask(now, ago(70), ago(65)));
        assert!(!may_ask(now, ago(30), None), "Kiln asked a moment ago");
        assert!(
            !may_ask(now, None, ago(30)),
            "the CLI would answer from its snapshot"
        );
        assert!(
            !may_ask(now, None, Some(now + Duration::from_secs(5))),
            "a clock ahead"
        );
    }
}
