//! What the conversation spent: the agent's own count, which comes as a turn
//! ends, and the requests since, so the total grows during a long turn.

use vortex_agent::{AgentEvent, SpentTokens};

#[derive(Default)]
pub struct Spent {
    reported: SpentTokens,
    /// The requests since. Subagents' are not reported one by one; they
    /// join with the next report.
    since: SpentTokens,
}

impl Spent {
    pub fn record(&mut self, event: &AgentEvent) {
        match event {
            AgentEvent::SpentTokens(spent) => {
                self.reported = *spent;
                self.since = SpentTokens::default();
            }
            AgentEvent::PromptTokens(prompt) => {
                self.since += SpentTokens {
                    input: prompt.uncached,
                    cached: prompt.cached,
                    written: prompt.written,
                    output: 0,
                };
            }
            AgentEvent::OutputTokens(output) => self.since.output += output,
            _ => {}
        }
    }

    pub fn so_far(&self) -> Option<SpentTokens> {
        let mut spent = self.reported;
        spent += self.since;
        (spent.total() > 0).then_some(spent)
    }
}

#[cfg(test)]
mod tests {
    use vortex_agent::PromptTokens;

    use super::*;

    #[test]
    fn requests_count_until_the_agent_reports() {
        let mut spent = Spent::default();
        assert_eq!(spent.so_far(), None);
        let prompt = PromptTokens {
            cached: 100,
            written: 20,
            uncached: 3,
            kept_for: None,
        };
        spent.record(&AgentEvent::PromptTokens(prompt));
        spent.record(&AgentEvent::OutputTokens(7));
        assert_eq!(spent.so_far().map(SpentTokens::total), Some(130));
        let reported = SpentTokens {
            output: 500,
            ..SpentTokens::default()
        };
        spent.record(&AgentEvent::SpentTokens(reported));
        assert_eq!(spent.so_far(), Some(reported));
        spent.record(&AgentEvent::OutputTokens(5));
        assert_eq!(spent.so_far().map(SpentTokens::total), Some(505));
    }
}
