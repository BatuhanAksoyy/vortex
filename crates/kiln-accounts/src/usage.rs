//! The plan usage each added account last reported. Only a reply carries an
//! added account's usage, so without it a chat would show none after a
//! restart until the account's first reply.

use std::{collections::HashMap, path::Path, time::SystemTime};

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
