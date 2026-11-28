//! What happens to the conversation as a whole: emptied, or compacted, which
//! the agent's own transcript no longer shows, so a line says so.

use vortex_agent::{AgentEvent, Compaction};

use super::{Change, Entry, Transcript};
use crate::agent_state::tokens;

impl Transcript {
    pub(super) fn reshape(&mut self, event: &AgentEvent) -> Change {
        match *event {
            AgentEvent::ConversationReset => {
                self.clear();
                Change::default()
            }
            AgentEvent::Compacting(compacting) => {
                self.compacting = compacting;
                Change::default()
            }
            AgentEvent::Compacted(compaction) => {
                self.compacting = false;
                self.push(Entry::Notice {
                    text: line(compaction),
                    is_error: false,
                })
            }
            AgentEvent::CompactFailed => {
                self.compacting = false;
                self.push(Entry::Notice {
                    text: "The conversation could not be compacted".into(),
                    is_error: true,
                })
            }
            _ => Change::default(),
        }
    }
}

/// `Conversation compacted · 180k → 12k tokens`.
fn line(compaction: Compaction) -> String {
    let how = if compaction.automatic {
        "Context full: conversation compacted automatically"
    } else {
        "Conversation compacted"
    };
    let size = match compaction.after {
        Some(after) => format!("{} → {} tokens", tokens(compaction.before), tokens(after)),
        None => format!("{} tokens summarized", tokens(compaction.before)),
    };
    format!("{how} · {size}")
}

#[cfg(test)]
mod tests {
    use vortex_agent::TurnOutcome;

    use super::*;

    #[test]
    fn a_compaction_shows_while_it_runs_and_leaves_a_line() {
        let mut transcript = Transcript::default();
        transcript.apply(AgentEvent::Compacting(true));
        transcript.apply(AgentEvent::Compacting(true));
        assert!(transcript.compacting());
        transcript.apply(AgentEvent::Compacting(false));
        transcript.apply(AgentEvent::Compacted(Compaction {
            automatic: true,
            before: 180_400,
            after: Some(12_000),
        }));
        assert!(!transcript.compacting());
        assert!(matches!(
            transcript.entry(0),
            Some((_, Entry::Notice { text, is_error: false }))
                if text == "Context full: conversation compacted automatically · 180.4k → 12k tokens"
        ));
        transcript.apply(AgentEvent::Compacting(true));
        transcript.apply(AgentEvent::TurnEnded(TurnOutcome::Interrupted));
        assert!(!transcript.compacting(), "an ended turn compacts no more");
    }
}
