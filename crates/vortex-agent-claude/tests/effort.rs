//! Replays the captured effort changes, and the prompt cache use a turn
//! reports as each request starts.

// Inside a `cfg(test)` module so helpers may `unwrap`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod effort {
    use std::time::Duration;

    use serde_json::Value;
    use vortex_agent::{AgentEvent, Effort, PromptTokens, SessionCommand};
    use vortex_agent_claude::Translator;

    use super::common::{fixture_path, output_lines};

    fn sent(name: &str) -> Vec<Value> {
        std::fs::read_to_string(fixture_path(name))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .filter(|record| record["dir"] == "in")
            .map(|record| record["msg"].clone())
            .collect()
    }

    fn events(translator: &mut Translator, name: &str) -> Vec<AgentEvent> {
        output_lines(name)
            .iter()
            .flat_map(|line| translator.on_line(line))
            .collect()
    }

    #[test]
    fn the_effort_is_read_and_set_for_the_session() {
        let mut translator = Translator::default();
        let commands = [
            SessionCommand::RefreshEffort,
            SessionCommand::SetEffort(Effort::Max),
            SessionCommand::RefreshEffort,
            SessionCommand::SetEffort(Effort::Low),
            SessionCommand::RefreshEffort,
        ];
        let recorded = sent("effort");
        for command in commands {
            let request = translator.on_command(command).unwrap();
            assert!(recorded.contains(&request), "{request} was not recorded");
        }
        let events = events(&mut translator, "effort");
        let efforts: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                AgentEvent::EffortChanged(effort) => Some(*effort),
                _ => None,
            })
            .collect();
        let (medium, max, low) = (Some(Effort::Medium), Some(Effort::Max), Some(Effort::Low));
        assert_eq!(efforts, [medium, max, max, low, low]);
        let Some(AgentEvent::Ready(info)) = events.first() else {
            panic!("expected the initialize reply first");
        };
        assert!(
            info.models
                .iter()
                .all(|model| model.efforts.contains(&Effort::Max)),
            "{:?}",
            info.models
        );
    }

    #[test]
    fn each_request_tells_how_its_prompt_was_cached() {
        let tokens: Vec<_> = events(&mut Translator::default(), "allow")
            .into_iter()
            .filter_map(|event| match event {
                AgentEvent::PromptTokens(tokens) => Some(tokens),
                _ => None,
            })
            .collect();
        let hour = Some(Duration::from_hours(1));
        assert_eq!(
            tokens,
            [
                PromptTokens {
                    cached: 13_248,
                    written: 12_024,
                    uncached: 2,
                    kept_for: hour,
                },
                PromptTokens {
                    cached: 25_272,
                    written: 6_662,
                    uncached: 2,
                    kept_for: hour,
                },
            ]
        );
    }
}
