//! Replays a subagent stopped while it ran a shell command, and a second
//! attempt to stop it.

// Inside a `cfg(test)` module so helpers may `unwrap`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod stop {
    use kiln_agent::{AgentEvent, SessionCommand, SubagentChange, SubagentEnd, SubagentEvent};
    use kiln_agent_claude::Translator;
    use serde_json::{Value, json};

    use super::common::{fixture_path, output_lines};

    fn sent(id: &str) -> Value {
        let text = std::fs::read_to_string(fixture_path("stop")).unwrap();
        text.lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .find(|record| record["dir"] == "in" && record["msg"]["request_id"] == id)
            .unwrap()["msg"]
            .clone()
    }

    /// The subagent's call, once it started.
    fn started(translator: &mut Translator, lines: &mut std::vec::IntoIter<String>) -> String {
        lines
            .by_ref()
            .flat_map(|line| translator.on_line(&line))
            .find_map(|event| match event {
                AgentEvent::Subagent(SubagentEvent {
                    call,
                    change: SubagentChange::Started { .. },
                }) => Some(call),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn a_stopped_subagent_reports_its_end() {
        let mut translator = Translator::default();
        let mut lines = output_lines("stop").into_iter();
        let call = started(&mut translator, &mut lines);
        for id in ["kiln-stop-1", "kiln-stop-2"] {
            let stop = translator.on_command(SessionCommand::StopSubagent(call.clone()));
            assert_eq!(stop.unwrap(), sent(id));
        }
        let events: Vec<_> = lines.flat_map(|line| translator.on_line(&line)).collect();
        let asked = events.iter().find_map(|event| match event {
            AgentEvent::PermissionRequest(request) => request.subagent.as_ref(),
            _ => None,
        });
        assert_eq!(asked, Some(&call), "the request names who asks");
        let ended = events.iter().any(|event| {
            matches!(event, AgentEvent::Subagent(SubagentEvent { call: of, change:
                SubagentChange::Finished { end: SubagentEnd::Stopped, .. } }) if *of == call)
        });
        assert!(ended);
        let failed = SubagentChange::StopFailed;
        assert!(
            !events.iter().any(
                |event| matches!(event, AgentEvent::Subagent(event) if event.change == failed)
            )
        );
    }

    #[test]
    fn a_refused_stop_is_reported_under_the_subagent() {
        let mut translator = Translator::default();
        let call = started(&mut translator, &mut output_lines("stop").into_iter());
        let stop = translator
            .on_command(SessionCommand::StopSubagent(call.clone()))
            .unwrap();
        let reply = json!({ "type": "control_response", "response": {
            "subtype": "error", "request_id": stop["request_id"], "error": "no such task",
        }});
        let change = SubagentChange::StopFailed;
        assert_eq!(
            translator.on_line(&reply.to_string()),
            [AgentEvent::Subagent(SubagentEvent { call, change })]
        );
        let unknown = SessionCommand::StopSubagent("other".into());
        assert_eq!(translator.on_command(unknown), None);
    }
}
