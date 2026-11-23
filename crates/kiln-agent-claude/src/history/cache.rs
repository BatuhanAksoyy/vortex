//! How a stored conversation's last request read its prompt, so a reopened
//! chat knows the state of the prompt cache before it sends anything.

use std::{path::PathBuf, time::SystemTime};

use kiln_agent::PromptTokens;
use serde::Deserialize;

use crate::{stream, wire::PromptUsage};

/// A model the CLI names for messages no model wrote, such as an API error;
/// their usage is all zeros.
const SYNTHETIC: &str = "<synthetic>";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Stored {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    is_sidechain: bool,
    timestamp: Option<String>,
    message: Option<StoredMessage>,
}

#[derive(Deserialize)]
struct StoredMessage {
    #[serde(default)]
    model: String,
    usage: Option<PromptUsage>,
}

/// The last request of conversation `id` and when its reply was written.
/// Subagents' requests are cached apart and skipped.
pub fn last_prompt_tokens(dirs: &[PathBuf], id: &str) -> Option<(PromptTokens, SystemTime)> {
    let text = super::stored(dirs, id).ok()?;
    text.lines().rev().find_map(last_request)
}

fn last_request(line: &str) -> Option<(PromptTokens, SystemTime)> {
    let stored: Stored = serde_json::from_str(line).ok()?;
    if stored.kind != "assistant" || stored.is_sidechain {
        return None;
    }
    let message = stored.message?;
    if message.model == SYNTHETIC {
        return None;
    }
    let at: jiff::Timestamp = stored.timestamp?.parse().ok()?;
    Some((stream::prompt_tokens(&message.usage?), SystemTime::from(at)))
}
