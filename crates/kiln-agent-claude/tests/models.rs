//! Replays the captured `list_models` reply: the models a chat offers once
//! it asks the agent again.

// Inside a `cfg(test)` module so helpers may `unwrap`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod models {
    use kiln_agent::{AgentEvent, ModelOption, SessionCommand};
    use kiln_agent_claude::Translator;

    use super::common::{fixture_path, output_lines};

    /// Asks for the list as the capture did, then replays the reply.
    fn listed() -> Vec<ModelOption> {
        let mut translator = Translator::default();
        let request = translator
            .on_command(SessionCommand::RefreshModels)
            .unwrap();
        let recorded = std::fs::read_to_string(fixture_path("models")).unwrap();
        let sent = recorded
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .any(|record| record["dir"] == "in" && record["msg"] == request);
        assert!(sent, "{request} was not recorded");
        output_lines("models")
            .iter()
            .flat_map(|line| translator.on_line(line))
            .find_map(|event| match event {
                AgentEvent::ModelsChanged(models) => Some(models),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn the_list_names_each_model_with_its_efforts() {
        let models = listed();
        let ids: Vec<&str> = models.iter().map(|model| model.id.as_str()).collect();
        assert_eq!(ids.first(), Some(&"default"));
        assert!(ids.contains(&"haiku"), "{ids:?}");
        let opus = models.iter().find(|model| model.id == "opus").unwrap();
        assert_eq!(opus.resolved, "claude-opus-5-5");
        assert_eq!(opus.name, "Opus");
        assert_eq!(opus.efforts.len(), 5, "low to max");
    }
}
