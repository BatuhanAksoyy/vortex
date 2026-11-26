use std::time::Duration;

use kiln_agent::{AgentEvent, BlockKey, PastMessage, ToolCall};

use super::*;

fn key(message: &str, index: u64) -> BlockKey {
    BlockKey {
        message_id: message.to_owned(),
        index,
    }
}

fn text(message: &str, index: u64, words: &str) -> AgentEvent {
    AgentEvent::TextDelta {
        block: key(message, index),
        text: words.to_owned(),
    }
}

fn thought(message: &str, words: &str) -> AgentEvent {
    AgentEvent::ThinkingDelta {
        block: key(message, 0),
        text: words.to_owned(),
    }
}

fn tool(id: &str) -> AgentEvent {
    AgentEvent::ToolCall(ToolCall {
        id: id.to_owned(),
        parent: None,
        name: "Read".to_owned(),
        input: serde_json::Value::Null,
    })
}

fn took(seconds: u64) -> AgentEvent {
    AgentEvent::TurnTook {
        duration: Duration::from_secs(seconds),
        ended_at: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1_000)),
    }
}

/// Notes kept over `events`, as they come back after a restart.
fn kept(events: &[AgentEvent]) -> KeptNotes {
    let dir = tempfile::tempdir().unwrap();
    let mut notes = KeptNotes::default();
    notes.keep_in(dir.path().to_owned());
    notes.follow("session-1");
    for event in events {
        notes.record(event);
    }
    let (path, contents) = notes.take_changes().unwrap();
    std::fs::write(&path, contents).unwrap();
    let mut reopened = KeptNotes::default();
    reopened.restore("session-1", &KeptNotes::read(Some(dir.path()), "session-1"));
    reopened
}

fn agent(events: Vec<AgentEvent>) -> Vec<PastMessage> {
    events.into_iter().map(PastMessage::Agent).collect()
}

#[test]
fn a_thought_the_agent_did_not_store_comes_back_before_what_followed_it() {
    let notes = kept(&[
        AgentEvent::ThinkingTokens(40),
        thought("msg_1", "Read the file first."),
        AgentEvent::ThinkingTime {
            message_id: "msg_1".into(),
            duration: Duration::from_secs(3),
        },
        tool("toolu_1"),
    ]);
    let stored = vec![
        PastMessage::User("Look".into()),
        PastMessage::Agent(tool("toolu_1")),
    ];
    let merged = notes.merge(stored);
    assert_eq!(
        merged[1..],
        agent(vec![
            thought("msg_1", "Read the file first."),
            AgentEvent::ThinkingTokens(40),
            AgentEvent::ThinkingTime {
                message_id: "msg_1".into(),
                duration: Duration::from_secs(3),
            },
            tool("toolu_1"),
        ])[..]
    );
}

#[test]
fn a_stored_thought_gets_only_its_tokens_and_time() {
    let notes = kept(&[thought("msg_1", "Hm."), AgentEvent::ThinkingTokens(9)]);
    let merged = notes.merge(agent(vec![thought("msg_1", "Hm.")]));
    assert_eq!(
        merged,
        agent(vec![thought("msg_1", "Hm."), AgentEvent::ThinkingTokens(9)])
    );
}

#[test]
fn turn_notes_end_their_turn_unless_the_agent_stored_them() {
    let notes = kept(&[
        text("msg_1", 0, "Done."),
        took(12),
        AgentEvent::Recap("Fixed the build.".into()),
        text("msg_2", 0, "Again."),
        took(5),
    ]);
    let stored = vec![
        PastMessage::User("Fix it".into()),
        PastMessage::Agent(text("msg_1", 0, "Done.")),
        PastMessage::User("Once more".into()),
        PastMessage::Agent(text("msg_2", 0, "Again.")),
        PastMessage::Agent(took(5)),
    ];
    let merged = notes.merge(stored);
    assert_eq!(
        merged[2..4],
        agent(vec![took(12), AgentEvent::Recap("Fixed the build.".into())])[..]
    );
    let durations = merged
        .iter()
        .filter(|message| matches!(message, PastMessage::Agent(AgentEvent::TurnTook { .. })))
        .count();
    assert_eq!(durations, 2);
}

#[test]
fn a_recap_the_agent_stored_shows_once() {
    let notes = kept(&[
        text("msg_1", 0, "Done."),
        AgentEvent::Recap("Fixed it.".into()),
    ]);
    let stored = agent(vec![
        text("msg_1", 0, "Done."),
        AgentEvent::Recap("Fixed it.".into()),
    ]);
    assert_eq!(notes.merge(stored.clone()), stored);
}

#[test]
fn a_session_id_never_names_a_file_outside_the_folder() {
    let dir = Path::new("/notes");
    assert_eq!(
        file(dir, "6f1c-2a"),
        Some(PathBuf::from("/notes/6f1c-2a.json"))
    );
    assert_eq!(file(dir, "../secrets"), None);
    assert_eq!(file(dir, ""), None);
}
