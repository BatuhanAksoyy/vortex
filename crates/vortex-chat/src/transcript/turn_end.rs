//! The lines that sum up: how long a turn took, and where the conversation
//! stands for a user coming back to it.

use std::time::{Duration, SystemTime};

use vortex_agent::AgentEvent;

use super::{Change, Transcript};
use crate::clock;

const VERBS: &[&str] = &[
    "Brewed",
    "Pondered",
    "Simmered",
    "Crafted",
    "Percolated",
    "Tinkered",
    "Mulled",
    "Worked",
];

impl Transcript {
    pub(super) fn sum_up(&mut self, event: AgentEvent) -> Change {
        match event {
            AgentEvent::TurnTook { duration, ended_at } => {
                let now = SystemTime::now();
                let line = done(self.turns(), duration, ended_at.unwrap_or(now), now);
                self.push_notice(line)
            }
            AgentEvent::Recap(text) => self.push_notice(format!("※ Recap: {text}")),
            _ => Change::default(),
        }
    }
}

/// `✻ Brewed for 4m 36s · done 16:28`; the verb changes with the turn.
fn done(turn: usize, took: Duration, at: SystemTime, now: SystemTime) -> String {
    let verb = VERBS[turn % VERBS.len()];
    let (took, at) = (clock::took(took), clock::reset_label(at, now));
    format!("✻ {verb} for {took} · done {at}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::Entry;

    #[test]
    fn a_turn_ends_with_how_long_it_took() {
        let mut transcript = Transcript::default();
        transcript.apply(AgentEvent::TurnTook {
            duration: Duration::from_secs(276),
            ended_at: None,
        });
        transcript.apply(AgentEvent::Recap("Next, run the tests.".into()));
        let lines: Vec<&str> = (0..transcript.len())
            .filter_map(|row| match transcript.entry(row) {
                Some((_, Entry::Notice { text, .. })) => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            lines[0].starts_with("✻ Brewed for 4m 36s · done "),
            "{lines:?}"
        );
        assert_eq!(lines[1], "※ Recap: Next, run the tests.");
    }
}
