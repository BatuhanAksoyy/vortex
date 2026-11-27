//! The recap Vortex asks for with the CLI's `/recap` command. The CLI runs it
//! like any message, but for the user it is no turn: its result ends none.

use serde_json::Value;
use vortex_agent::AgentEvent;

use crate::{outgoing, wire};

const COMMAND: &str = "recap";

#[derive(Default)]
pub struct Recaps {
    /// The user turned recaps off in the CLI's settings.
    off: bool,
    /// Asked for, and its output has not come yet.
    asked: bool,
    /// Its output came; the next result is its own.
    answered: bool,
}

impl Recaps {
    pub fn ask(&mut self) -> Option<Value> {
        if self.off {
            return None;
        }
        self.asked = true;
        Some(outgoing::user_message(&format!("/{COMMAND}").into()))
    }

    /// Follows the user's choice in a `get_settings` reply.
    pub fn follow(&mut self, settings: &Value) {
        self.off = settings["effective"]["awaySummaryEnabled"] == false;
    }

    /// The recap `message` is, also one the user typed the command for.
    pub fn output(&mut self, message: &wire::Assistant) -> Option<AgentEvent> {
        let run = message.local_command_run.as_ref()?;
        if run.command != COMMAND {
            return None;
        }
        self.answered = std::mem::take(&mut self.asked);
        let text = message
            .message
            .content
            .iter()
            .find_map(|block| match block {
                wire::AssistantBlock::Text { text } if !text.is_empty() => Some(text.clone()),
                _ => None,
            })?;
        Some(AgentEvent::Recap(text))
    }

    /// Whether the result now read ends a recap Vortex asked for. Any result
    /// settles the request: one that failed must not swallow a later turn's.
    pub fn ends(&mut self) -> bool {
        self.asked = false;
        std::mem::take(&mut self.answered)
    }
}
