//! The lines the CLI adds to a stored conversation itself that a reopened
//! chat shows: how long a turn took, and recaps.

use std::time::{Duration, SystemTime};

use serde::Deserialize;
use serde_json::Value;
use vortex_agent::{AgentEvent, PastMessage};

use super::messages::tagged;

/// The command that writes a recap.
pub const RECAP: &str = "/recap";

/// The CLI ends the recaps it writes unasked with this.
const HOW_TO_STOP: &str = "(disable recaps in /config)";

#[derive(Deserialize)]
#[serde(tag = "subtype", rename_all = "snake_case")]
pub enum Note {
    #[serde(rename_all = "camelCase")]
    TurnDuration {
        duration_ms: u64,
        timestamp: Option<String>,
    },
    AwaySummary {
        content: String,
    },
    /// The output of a slash command the CLI ran.
    LocalCommand {
        #[serde(default)]
        content: String,
    },
    #[serde(other)]
    Other,
}

impl Note {
    /// How the note after the line `before` is shown.
    pub fn shown(self, before: Option<&Value>) -> Option<PastMessage> {
        let event = match self {
            Self::TurnDuration {
                duration_ms,
                timestamp,
            } => AgentEvent::TurnTook {
                duration: Duration::from_millis(duration_ms),
                ended_at: timestamp
                    .and_then(|at| at.parse::<jiff::Timestamp>().ok())
                    .map(SystemTime::from),
            },
            Self::AwaySummary { content } => {
                AgentEvent::Recap(content.replace(HOW_TO_STOP, "").trim().to_owned())
            }
            Self::LocalCommand { content } if before.is_some_and(runs_recap) => {
                AgentEvent::Recap(tagged(&content, "local-command-stdout")?.to_owned())
            }
            Self::LocalCommand { .. } | Self::Other => return None,
        };
        Some(PastMessage::Agent(event))
    }
}

fn runs_recap(line: &Value) -> bool {
    line["message"]["content"]
        .as_str()
        .and_then(|text| tagged(text, "command-name"))
        == Some(RECAP)
}

/// Whether `line` is a note of the main conversation's.
pub fn is_note(line: &Value) -> bool {
    line["type"] == "system"
        && line["isSidechain"] != true
        && matches!(
            line["subtype"].as_str(),
            Some("turn_duration" | "away_summary" | "local_command")
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::messages::messages;

    /// A turn and a recap stored by `claude -p` (capture.py recap).
    const RECAPPED: &str = include_str!("../../tests/fixtures/recap-session.jsonl");

    #[test]
    fn a_recap_is_shown_without_its_command() {
        let messages = messages(RECAPPED);
        assert!(
            !messages.iter().any(
                |message| matches!(message, PastMessage::User(sent) if sent.text.contains("recap"))
            ),
            "{messages:#?}"
        );
        assert!(
            matches!(messages.last(), Some(PastMessage::Agent(AgentEvent::Recap(text))) if text.starts_with("The overall goal")),
            "{messages:#?}"
        );
    }

    #[test]
    fn the_clis_own_notes_are_shown() {
        let stored = [
        r#"{"type":"user","uuid":"a","parentUuid":null,"message":{"role":"user","content":"hi"}}"#,
        r#"{"type":"system","subtype":"turn_duration","uuid":"b","parentUuid":"a","durationMs":84578,"timestamp":"2026-11-17T13:20:01.338Z"}"#,
        r#"{"type":"system","subtype":"away_summary","uuid":"c","parentUuid":"b","content":"All done. (disable recaps in /config)"}"#,
    ]
    .join("\n");
        let messages = messages(&stored);
        assert!(
            matches!(
                &messages[1..],
                [
                    PastMessage::Agent(AgentEvent::TurnTook { duration, ended_at: Some(_) }),
                    PastMessage::Agent(AgentEvent::Recap(text)),
                ] if duration.as_millis() == 84_578 && text == "All done."
            ),
            "{messages:#?}"
        );
    }
}
