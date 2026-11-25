//! Replays a turn whose thinking comes as a summary, with estimates of its
//! tokens as it goes and how long it took at the end.

// Inside a `cfg(test)` module so helpers may `unwrap`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod thinking {
    use std::time::Duration;

    use kiln_agent::{AgentEvent, SpentTokens};
    use kiln_agent_claude::Translator;

    use super::common::output_lines;

    #[test]
    fn a_thought_comes_with_its_words_tokens_and_time() {
        let mut translator = Translator::default();
        let events: Vec<AgentEvent> = output_lines("thinking")
            .iter()
            .flat_map(|line| translator.on_line(line))
            .collect();
        let words: String = events
            .iter()
            .filter_map(|event| match event {
                AgentEvent::ThinkingDelta { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(words.starts_with("I'm counting the r's"), "{words}");
        let tokens: u64 = events
            .iter()
            .filter_map(|event| match event {
                AgentEvent::ThinkingTokens(tokens) => Some(tokens),
                _ => None,
            })
            .sum();
        assert_eq!(tokens, 214);
        assert!(events.contains(&AgentEvent::OutputTokens(123)));
        let spent = SpentTokens {
            input: 2,
            cached: 10_540,
            written: 10_493,
            output: 123,
        };
        assert!(events.contains(&AgentEvent::SpentTokens(spent)));
        let block = events
            .iter()
            .find_map(|event| match event {
                AgentEvent::ThinkingDelta { block, .. } => Some(block.message_id.clone()),
                _ => None,
            })
            .unwrap();
        assert!(events.contains(&AgentEvent::ThinkingTime {
            message_id: block,
            duration: Duration::from_millis(1154),
        }));
    }
}
