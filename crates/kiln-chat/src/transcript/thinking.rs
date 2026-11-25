//! What the model's thinking cost: about how many tokens it took, counted
//! as it goes, and how long, once it is done.

use std::time::Duration;

use super::{Change, Entry, Transcript};

impl Transcript {
    /// Counts `tokens` to the thought under way. They can come before its
    /// first words, which start its row.
    pub(super) fn count_thinking(&mut self, tokens: u64) -> Change {
        let row = self.entries.len().saturating_sub(1);
        if let Some((
            _,
            Entry::Thinking {
                tokens: counted,
                took: None,
                ..
            },
        )) = self.entries.last_mut()
        {
            *counted += tokens;
            return Change {
                updated: Some(row),
                appended: 0,
            };
        }
        self.unclaimed_thinking += tokens;
        Change::default()
    }

    pub(super) fn time_thinking(&mut self, message_id: &str, duration: Duration) -> Change {
        let row = self.entries.iter().rposition(|(_, entry)| {
            matches!(entry, Entry::Thinking { block, .. } if block.message_id == message_id)
        });
        let Some(row) = row else {
            return Change::default();
        };
        if let Some((_, Entry::Thinking { took, .. })) = self.entries.get_mut(row) {
            *took = Some(duration);
        }
        Change {
            updated: Some(row),
            appended: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use kiln_agent::{AgentEvent, BlockKey};

    use super::*;

    #[test]
    fn a_thought_counts_its_tokens_and_takes_its_time() {
        let mut transcript = Transcript::default();
        let block = BlockKey {
            message_id: "m1".into(),
            index: 0,
        };
        let words = |text: &str| AgentEvent::ThinkingDelta {
            block: block.clone(),
            text: text.into(),
        };
        // The first estimate comes before the first words.
        transcript.apply(AgentEvent::ThinkingTokens(5));
        transcript.apply(words("Counting"));
        transcript.apply(AgentEvent::ThinkingTokens(25));
        transcript.apply(words(" the letters."));
        transcript.apply(AgentEvent::ThinkingTime {
            message_id: "m1".into(),
            duration: Duration::from_millis(1154),
        });
        // Thinking done, more estimates belong to the next thought.
        transcript.apply(AgentEvent::ThinkingTokens(7));
        let Some((
            _,
            Entry::Thinking {
                text, tokens, took, ..
            },
        )) = transcript.entry(0)
        else {
            panic!("no thought");
        };
        assert_eq!(text, "Counting the letters.");
        assert_eq!((*tokens, *took), (30, Some(Duration::from_millis(1154))));
        assert_eq!(transcript.unclaimed_thinking, 7);
    }
}
