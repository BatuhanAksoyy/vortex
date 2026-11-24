//! Replays a turn whose message carried a picture and mentioned a file:
//! the message goes out as the CLI took it, and a picture the Read tool
//! returned comes back with its result.

// Inside a `cfg(test)` module so helpers may `unwrap`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod attach {
    use std::path::PathBuf;

    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use kiln_agent::{AgentEvent, Message, Picture, SessionCommand};
    use kiln_agent_claude::Translator;
    use serde_json::{Value, json};

    use super::common::{fixture_path, output_lines};

    /// The message the capture sent, as it went to the CLI.
    fn sent() -> Value {
        std::fs::read_to_string(fixture_path("attach"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .find(|record| record["dir"] == "in" && record["msg"]["type"] == "user")
            .map(|record| record["msg"].clone())
            .unwrap()
    }

    #[test]
    fn a_picture_goes_as_the_cli_took_it_before_the_text_and_its_mentions() {
        let sent = sent();
        let captured = &sent["message"]["content"][0];
        let data = STANDARD
            .decode(captured["source"]["data"].as_str().unwrap())
            .unwrap();
        let message = Message {
            text: "Which colour?".into(),
            pictures: vec![Picture {
                media_type: "image/png".into(),
                data: data.into(),
            }],
            files: vec![PathBuf::from("note.txt")],
        };
        let mut translator = Translator::default();
        let out = translator
            .on_command(SessionCommand::Send(message))
            .unwrap();
        assert_eq!(out["message"]["content"][0], *captured);
        assert_eq!(
            out["message"]["content"][1],
            json!({ "type": "text", "text": "Which colour?\n\n@note.txt" })
        );
    }

    #[test]
    fn plain_text_goes_as_a_string() {
        let mut translator = Translator::default();
        let out = translator
            .on_command(SessionCommand::Send("hi".into()))
            .unwrap();
        assert_eq!(out["message"]["content"], "hi");
    }

    #[test]
    fn a_file_with_a_space_is_mentioned_in_quotes() {
        let message = Message {
            text: String::new(),
            pictures: Vec::new(),
            files: vec![PathBuf::from("/tmp/my notes.txt")],
        };
        let mut translator = Translator::default();
        let out = translator
            .on_command(SessionCommand::Send(message))
            .unwrap();
        assert_eq!(out["message"]["content"], "@\"/tmp/my notes.txt\"");
    }

    #[test]
    fn a_picture_the_read_tool_returned_comes_with_its_result() {
        let mut translator = Translator::default();
        let pictures: Vec<Picture> = output_lines("attach")
            .iter()
            .flat_map(|line| translator.on_line(line))
            .flat_map(|event| match event {
                AgentEvent::ToolResult { pictures, .. } => pictures,
                _ => Vec::new(),
            })
            .collect();
        assert_eq!(pictures.len(), 1);
        assert_eq!(pictures[0].media_type, "image/png");
        assert!(pictures[0].data.starts_with(b"\x89PNG"));
    }
}
