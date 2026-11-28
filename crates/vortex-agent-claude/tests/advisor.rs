//! Replays a turn whose model consults the advisor, a tool the API runs
//! within the model's message.

// Inside a `cfg(test)` module so helpers may `unwrap`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod advisor {
    use vortex_agent::AgentEvent;
    use vortex_agent_claude::Translator;

    use super::common::output_lines;

    #[test]
    fn the_advisor_shows_as_a_tool_call_with_its_result() {
        let mut translator = Translator::default();
        let events: Vec<AgentEvent> = output_lines("advisor")
            .iter()
            .flat_map(|line| translator.on_line(line))
            .collect();
        let call = events
            .iter()
            .position(|event| matches!(event, AgentEvent::ToolCall(call) if call.name == "advisor"))
            .unwrap();
        let AgentEvent::ToolCall(called) = &events[call] else {
            unreachable!()
        };
        assert_eq!(called.parent, None);
        let result = events
            .iter()
            .position(
                |event| matches!(event, AgentEvent::ToolResult { id, .. } if *id == called.id),
            )
            .unwrap();
        assert!(call < result, "{events:#?}");
        let AgentEvent::ToolResult {
            output, is_error, ..
        } = &events[result]
        else {
            unreachable!()
        };
        assert_eq!(
            (output.as_str(), *is_error),
            ("", false),
            "the advice is encrypted"
        );
    }
}
