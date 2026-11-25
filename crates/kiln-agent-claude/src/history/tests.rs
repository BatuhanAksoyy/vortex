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
            PastMessage::User(message) => Some(message.text.as_str()),
            PastMessage::Agent(AgentEvent::TextDelta { text, .. }) => Some(text.as_str()),
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

#[test]
fn pictures_sent_with_a_message_and_read_by_a_tool_come_back() {
    let red = include_str!("../../tests/fixtures/attach.jsonl")
        .lines()
        .find(|line| line.contains("\"dir\": \"in\"") && line.contains("\"image\""))
        .and_then(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .map_or_default(|record| record["msg"].clone());
    let stored = serde_json::json!({
        "type": "user",
        "uuid": "u1",
        "message": red["message"],
    });
    let messages = messages(&stored.to_string());
    let [PastMessage::User(message)] = messages.as_slice() else {
        panic!("{messages:?}");
    };
    assert!(message.text.starts_with("Name the colour"));
    assert_eq!(message.pictures.len(), 1);
    assert_eq!(message.pictures[0].media_type, "image/png");
    assert!(message.pictures[0].data.starts_with(b"\x89PNG"));
}

/// A conversation whose subagent was sent a second task with `SendMessage`
/// (capture.py followup), and the files the CLI stored for the subagent.
const FOLLOWUP: &str = include_str!("../../tests/fixtures/followup-session.jsonl");
const AGENT_ID: &str = "a84f85d1bb03e9c2c";
const AGENT: &str = include_str!("../../tests/fixtures/followup-agent-a84f85d1bb03e9c2c.jsonl");
const AGENT_META: &str =
    include_str!("../../tests/fixtures/followup-agent-a84f85d1bb03e9c2c.meta.json");

fn followup() -> Vec<PastMessage> {
    reopened(FOLLOWUP, &[(AGENT_ID, AGENT_META, AGENT)])
}

/// Conversation `s-1` reopened from `session` and the files of `agents`,
/// each its id, `.meta.json` and conversation, laid out as the CLI does.
pub(super) fn reopened(session: &str, agents: &[(&str, &str, &str)]) -> Vec<PastMessage> {
    let dir = tempfile::tempdir().unwrap();
    let stored = dir.path().join("s-1").join("subagents");
    std::fs::create_dir_all(&stored).unwrap();
    std::fs::write(dir.path().join("s-1.jsonl"), session).unwrap();
    for (id, meta, text) in agents {
        std::fs::write(stored.join(format!("agent-{id}.meta.json")), meta).unwrap();
        std::fs::write(stored.join(format!("agent-{id}.jsonl")), text).unwrap();
    }
    read(&[dir.path().to_owned()], "s-1").unwrap()
}

/// How each subagent's card ends, by the call that started it.
pub(super) fn ends(messages: &[PastMessage]) -> Vec<(String, kiln_agent::SubagentEnd, String)> {
    messages
        .iter()
        .filter_map(|message| match message {
            PastMessage::Agent(AgentEvent::Subagent(kiln_agent::SubagentEvent {
                call,
                change: kiln_agent::SubagentChange::Finished { end, summary, .. },
            })) => Some((call.clone(), *end, summary.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn a_stored_subagent_fills_its_card_in_the_order_it_ran() {
    use kiln_agent::{SubagentChange, SubagentEvent};
    let messages = followup();
    let position = |wanted: &dyn Fn(&AgentEvent) -> bool| {
        messages
            .iter()
            .position(|message| matches!(message, PastMessage::Agent(event) if wanted(event)))
            .unwrap()
    };
    let agent = "toolu_01KeBu2QBL3ymTYqfr3kSWKy";
    let change = |of: &AgentEvent| match of {
        AgentEvent::Subagent(SubagentEvent { call, change }) => {
            assert_eq!(call, agent);
            Some(change.clone())
        }
        _ => None,
    };
    let started = position(
        &|event| matches!(change(event), Some(SubagentChange::Started { description, .. }) if description == "Read the note"),
    );
    let read = position(
        &|event| matches!(event, AgentEvent::ToolCall(call) if call.name == "Read" && call.parent.as_deref() == Some(agent)),
    );
    let lantern = position(
        &|event| matches!(change(event), Some(SubagentChange::Finished { summary, .. }) if summary == "lantern"),
    );
    let send = position(
        &|event| matches!(event, AgentEvent::ToolCall(call) if call.name == "SendMessage"),
    );
    let resumed = position(&|event| {
        change(event)
            == Some(SubagentChange::Resumed {
                message: "Reply with the second word of note.txt.".into(),
            })
    });
    let moth = position(&|event| change(event) == Some(SubagentChange::Text("moth".into())));
    let report = position(
        &|event| matches!(change(event), Some(SubagentChange::Finished { summary, usage: Some(_), .. }) if summary == "moth"),
    );
    assert!(started < read && read < lantern, "{messages:#?}");
    assert!(lantern < send && send < resumed && resumed < moth && moth < report);
    assert!(
        texts(&messages)
            .iter()
            .all(|text| !text.contains("task-notification")),
        "reports are not shown as messages"
    );
}

#[test]
fn stored_subagent_fixtures_hold_no_personal_data() {
    for text in [FOLLOWUP, AGENT, AGENT_META] {
        for needle in ["/home/", "/Users/", "/tmp/", "@"] {
            assert!(!text.contains(needle), "contains {needle:?}");
        }
    }
}

/// Two subagents started in one turn. The first reported while the turn
/// still ran, so its report is stored as a queued command; the second
/// stored no report at all.
const QUEUED: &str = r#"{"type":"user","uuid":"u1","parentUuid":null,"message":{"role":"user","content":"go"}}
{"type":"assistant","uuid":"a1","parentUuid":"u1","message":{"content":[{"type":"tool_use","id":"call-1","name":"Agent","input":{}},{"type":"tool_use","id":"call-2","name":"Agent","input":{}}]}}
{"type":"user","uuid":"r1","parentUuid":"a1","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"call-1","content":"launched"},{"type":"tool_result","tool_use_id":"call-2","content":"launched"}]}}
{"type":"attachment","uuid":"q1","parentUuid":"r1","attachment":{"type":"queued_command","prompt":"<task-notification>\n<task-id>agent1</task-id>\n<tool-use-id>call-1</tool-use-id>\n<status>completed</status>\n<summary>Agent \"One\" finished</summary>\n<result>lantern</result>\n</task-notification>"}}
{"type":"assistant","uuid":"a2","parentUuid":"q1","message":{"content":[{"type":"text","text":"done"}]}}
"#;
const AGENT_TEXT: &str = r#"{"type":"assistant","isSidechain":true,"message":{"model":"claude-haiku-5-5","content":[{"type":"text","text":"moth"}]}}"#;

#[test]
fn a_report_queued_during_a_turn_ends_its_card() {
    use kiln_agent::SubagentEnd;
    let one = r#"{"agentType":"general-purpose","description":"One","toolUseId":"call-1","finished":true}"#;
    let two = r#"{"agentType":"general-purpose","description":"Two","toolUseId":"call-2","finished":true}"#;
    let messages = reopened(
        QUEUED,
        &[("agent1", one, AGENT_TEXT), ("agent2", two, AGENT_TEXT)],
    );
    let mut ends = ends(&messages);
    ends.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        ends,
        [
            ("call-1".into(), SubagentEnd::Completed, "lantern".into()),
            ("call-2".into(), SubagentEnd::Completed, "moth".into()),
        ],
        "the second, marked finished, reports its last text"
    );
    assert_eq!(
        texts(&messages),
        ["go", "done"],
        "no report shows as a message"
    );
}

#[test]
fn a_subagent_that_never_finished_shows_stopped() {
    use kiln_agent::SubagentEnd;
    let two = r#"{"agentType":"general-purpose","description":"Two","toolUseId":"call-2"}"#;
    let messages = reopened(QUEUED, &[("agent2", two, AGENT_TEXT)]);
    let stopped = ends(&messages)
        .into_iter()
        .find(|(call, ..)| call == "call-2")
        .unwrap();
    assert_eq!(stopped.1, SubagentEnd::Stopped);
}
