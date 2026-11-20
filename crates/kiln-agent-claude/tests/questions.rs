//! Replays questions the model asks with `AskUserQuestion`: how they arrive,
//! and that answers go back exactly as the CLI accepted them.

// Inside a `cfg(test)` module so helpers may `unwrap`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod questions {
    use kiln_agent::{AgentEvent, Decision, PermissionRequest, SessionCommand};
    use kiln_agent_claude::Translator;

    use super::common::{fixture_path, output_lines};

    fn recorded_answer(name: &str) -> serde_json::Value {
        let text = std::fs::read_to_string(fixture_path(name)).unwrap();
        text.lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .filter(|record| record["dir"] == "in")
            .map(|record| record["msg"].clone())
            .find(|msg| msg["type"] == "control_response")
            .unwrap()
    }

    fn asked(name: &str) -> (Translator, PermissionRequest) {
        let mut translator = Translator::default();
        let request = output_lines(name)
            .iter()
            .flat_map(|line| translator.on_line(line))
            .find_map(|event| match event {
                AgentEvent::PermissionRequest(request) => Some(request),
                _ => None,
            })
            .unwrap();
        (translator, request)
    }

    fn answer(
        translator: &mut Translator,
        request_id: &str,
        answers: &[&str],
    ) -> serde_json::Value {
        translator
            .on_command(SessionCommand::Permission {
                request_id: request_id.into(),
                decision: Decision::Answer(answers.iter().map(|&a| a.to_owned()).collect()),
            })
            .unwrap()
    }

    #[test]
    fn a_question_arrives_with_its_options() {
        let (_, request) = asked("ask");
        assert_eq!(request.tool, "AskUserQuestion");
        let [question] = request.questions.as_slice() else {
            panic!("{request:#?}");
        };
        assert_eq!(question.text, "Which color do you prefer?");
        assert_eq!(question.header, "Color");
        assert!(!question.multi_select, "single choice");
        let labels: Vec<_> = question.options.iter().map(|o| o.label.as_str()).collect();
        assert_eq!(labels, ["Red", "Blue"]);
        assert_eq!(question.options[0].description, "Choose red");
    }

    #[test]
    fn an_answer_is_sent_as_recorded() {
        let (mut translator, request) = asked("ask");
        let sent = answer(&mut translator, &request.id, &["Red"]);
        assert_eq!(sent, recorded_answer("ask"));
    }

    #[test]
    fn several_choices_and_own_words_are_sent_as_recorded() {
        let (mut translator, request) = asked("questions");
        let multi: Vec<_> = request.questions.iter().map(|q| q.multi_select).collect();
        assert_eq!(multi, [true, false]);
        let sent = answer(&mut translator, &request.id, &["Apple, Banana", "Istanbul"]);
        assert_eq!(sent, recorded_answer("questions"));
    }

    #[test]
    fn other_tools_ask_no_questions() {
        let mut translator = Translator::default();
        let request = output_lines("allow")
            .iter()
            .flat_map(|line| translator.on_line(line))
            .find_map(|event| match event {
                AgentEvent::PermissionRequest(request) => Some(request),
                _ => None,
            })
            .unwrap();
        assert_eq!(request.questions, Vec::new());
    }
}
