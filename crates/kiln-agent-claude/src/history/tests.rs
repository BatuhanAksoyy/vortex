use std::time::SystemTime;

use kiln_agent::{AgentEvent, PastMessage, TurnOutcome};

use super::{messages::user_prompt, *};

/// Two turns stored by `claude -p` (capture.py history): the first reads
/// note.txt with the Read tool and replies with its first word.
const STORED: &str = include_str!("../../tests/fixtures/history.jsonl");

fn texts(messages: &[PastMessage]) -> Vec<&str> {
    messages
        .iter()
        .filter_map(|message| match message {
            PastMessage::User(text) | PastMessage::Agent(AgentEvent::TextDelta { text, .. }) => {
                Some(text.as_str())
            }
            PastMessage::Agent(_) => None,
        })
        .collect()
}

#[test]
fn a_stored_conversation_reads_as_its_messages() {
    let messages = messages(STORED);
    assert_eq!(
        texts(&messages),
        [
            "Read the file note.txt with the Read tool, then reply with its first word only.",
            "lantern",
            "Reply with the single word done.",
            "done",
        ]
    );
    let call = messages.iter().position(|message| {
        matches!(message, PastMessage::Agent(AgentEvent::ToolCall(call)) if call.name == "Read")
    });
    let result = messages.iter().position(|message| {
        matches!(message, PastMessage::Agent(AgentEvent::ToolResult { output, is_error: false, .. }) if output.contains("lantern"))
    });
    assert!(call.is_some() && call < result, "{messages:#?}");
}

#[test]
fn an_abandoned_branch_is_not_shown() {
    let edited = r#"{"type":"user","uuid":"b","parentUuid":"a2","message":{"role":"user","content":"second, edited"}}"#;
    let stored = format!(
        "{}\n{}\n{}\n{}\n",
        r#"{"type":"user","uuid":"a","parentUuid":null,"message":{"role":"user","content":"first"}}"#,
        r#"{"type":"assistant","uuid":"a2","parentUuid":"a","message":{"content":[{"type":"text","text":"one"}]}}"#,
        r#"{"type":"user","uuid":"old","parentUuid":"a2","message":{"role":"user","content":"second"}}"#,
        edited,
    );
    assert_eq!(
        texts(&messages(&stored)),
        ["first", "one", "second, edited"]
    );
}

#[test]
fn commands_and_interruptions_read_as_typed() {
    let line = |content: &str| serde_json::json!({"type": "user", "uuid": content, "message": {"role": "user", "content": content}});
    let command = "<command-name>/model</command-name>\n<command-message>model</command-message>\n<command-args>haiku</command-args>";
    assert_eq!(user_prompt(&line(command)), Some("/model haiku".into()));
    assert_eq!(
        user_prompt(&line("<local-command-stdout>Set</local-command-stdout>")),
        None
    );
    let pasted = "\n\n<pasted_content id=\"3f08\">\n# Notes\n</pasted_content>\n\ncontinue";
    assert_eq!(
        user_prompt(&line(pasted)),
        Some("# Notes\n\n\ncontinue".into())
    );
    let interrupted = format!("{}\n", line("[Request interrupted by user]"));
    assert_eq!(
        messages(&interrupted),
        [PastMessage::Agent(AgentEvent::TurnEnded(
            TurnOutcome::Interrupted
        ))]
    );
}

#[test]
fn the_title_follows_the_extension() {
    assert_eq!(
        title(STORED, STORED).as_deref(),
        Some("Reply with the single word done.")
    );
    let named = format!(
        "{STORED}{}\n",
        r#"{"type":"ai-title","aiTitle":"Reading a note","sessionId":"id-1"}"#
    );
    assert_eq!(title(&named, &named).as_deref(), Some("Reading a note"));
    let renamed = format!(
        "{named}{}\n",
        r#"{"type":"custom-title","customTitle":"Mine","sessionId":"id-1"}"#
    );
    assert_eq!(title(&renamed, &renamed).as_deref(), Some("Mine"));
    let first_only: String = STORED
        .lines()
        .filter(|line| !line.contains("last-prompt"))
        .flat_map(|line| [line, "\n"])
        .collect();
    assert_eq!(
        title(&first_only, &first_only).as_deref(),
        Some("Read the file note.txt with the Read tool, then reply with its first word only.")
    );
    let subagent = r#"{"type":"user","isSidechain":true,"message":{"role":"user","content":"x"}}"#;
    assert_eq!(title(subagent, subagent), None);
}

#[test]
fn a_project_directory_is_named_after_its_path() {
    let projects = Path::new("/c/projects");
    assert_eq!(
        project_dir(projects, Path::new("/Users/me/Git/kiln.rs")),
        Some(projects.join("-Users-me-Git-kiln-rs"))
    );
    assert_eq!(
        project_dir(projects, Path::new("/a/çay 😀")),
        Some(projects.join("-a--ay---")),
        "one dash per UTF-16 unit"
    );
}

#[test]
fn sessions_are_listed_newest_first() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("old.jsonl"), STORED).unwrap();
    std::fs::write(dir.path().join("notes.txt"), "not a session").unwrap();
    std::fs::write(dir.path().join("empty.jsonl"), "").unwrap();
    let newer = format!(
        "{}\n",
        r#"{"type":"user","uuid":"n","message":{"role":"user","content":"newer"}}"#
    );
    std::fs::write(dir.path().join("new.jsonl"), newer).unwrap();
    // Written since, without a message, as a resumed process does.
    let resumed = format!(
        "{STORED}{}\n",
        r#"{"type":"cost-state","sessionId":"id-1"}"#
    );
    std::fs::write(dir.path().join("reopened.jsonl"), resumed).unwrap();
    let sessions = list(&[dir.path().to_owned(), dir.path().join("missing")]).unwrap();
    let listed: Vec<(&str, &str)> = sessions
        .iter()
        .map(|s| (s.id.as_str(), s.title.as_str()))
        .collect();
    assert_eq!(listed[0], ("new", "newer"), "no message time: the file's");
    assert_eq!(listed.len(), 3, "{listed:?}");
    let last_message: jiff::Timestamp = "2026-11-23T14:42:26.958Z".parse().unwrap();
    let reopened = sessions.iter().find(|s| s.id == "reopened").unwrap();
    assert_eq!(reopened.updated_at, SystemTime::from(last_message));
    assert_eq!(
        read(&[dir.path().to_owned()], "old").unwrap(),
        messages(STORED)
    );
    assert!(read(&[dir.path().to_owned()], "gone").is_err());
}

#[test]
fn the_fixture_holds_no_personal_data() {
    for needle in ["/home/", "/Users/", "/tmp/", "@"] {
        assert!(!STORED.contains(needle), "history contains {needle:?}");
    }
}

/// `capture.py running`: a process's record, with its pid replaced.
const RUNNING: &str = include_str!("../../tests/fixtures/running-session.json");

#[test]
fn a_conversation_open_in_a_live_process_is_open_elsewhere() {
    let config = tempfile::tempdir().unwrap();
    let sessions = config.path().join("sessions");
    std::fs::create_dir(&sessions).unwrap();
    let record = |pid: u32, id: &str| {
        let mut running: serde_json::Value = serde_json::from_str(RUNNING).unwrap();
        running["pid"] = pid.into();
        running["sessionId"] = id.into();
        std::fs::write(sessions.join(format!("{pid}.json")), running.to_string()).unwrap();
    };
    let live = std::process::id();
    record(live, "live");
    // No process has the largest pid: a crash left this record behind.
    record(i32::MAX.unsigned_abs(), "crashed");
    std::fs::write(sessions.join("note.txt"), "not a record").unwrap();
    let open = open_elsewhere(config.path(), &std::collections::HashSet::new());
    assert_eq!(open, ["live".to_owned()].into());
    let own = [live].into();
    assert!(
        open_elsewhere(config.path(), &own).is_empty(),
        "Kiln's own process"
    );
    assert!(!RUNNING.contains("/Users/") && !RUNNING.contains('@'));
}

#[test]
fn the_last_request_tells_how_the_prompt_cache_was_left() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("old.jsonl"), STORED).unwrap();
    let (tokens, at) = last_prompt_tokens(&[dir.path().to_owned()], "old").unwrap();
    assert_eq!(
        tokens,
        kiln_agent::PromptTokens {
            cached: 23_879,
            written: 81,
            uncached: 2,
            kept_for: Some(std::time::Duration::from_hours(1)),
        }
    );
    let replied: jiff::Timestamp = "2026-11-23T14:42:26.958Z".parse().unwrap();
    assert_eq!(at, SystemTime::from(replied));
    assert_eq!(
        last_prompt_tokens(&[dir.path().to_owned()], "missing"),
        None
    );
}
