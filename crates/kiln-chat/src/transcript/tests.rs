use super::*;
use serde_json::json;

fn block(index: u64) -> BlockKey {
    BlockKey {
        message_id: "m1".into(),
        index,
    }
}

fn text(index: u64, text: &str) -> AgentEvent {
    AgentEvent::TextDelta {
        block: block(index),
        text: text.into(),
    }
}

fn call(id: &str) -> AgentEvent {
    AgentEvent::ToolCall(ToolCall {
        id: id.into(),
        parent: None,
        name: "Bash".into(),
        input: json!({ "command": "ls" }),
    })
}

fn permission(id: &str) -> AgentEvent {
    AgentEvent::PermissionRequest(PermissionRequest {
        id: id.into(),
        tool: "Bash".into(),
        input: json!({}),
        description: None,
        remember: None,
        questions: Vec::new(),
    })
}

#[test]
fn deltas_of_one_block_extend_one_entry() {
    let mut transcript = Transcript::default();
    assert_eq!(transcript.apply(text(0, "Hel")).appended, 1);
    assert_eq!(transcript.apply(text(0, "lo")).updated, Some(0));
    assert_eq!(transcript.apply(text(1, "Next")).appended, 1);
    assert_eq!(transcript.len(), 2);
    assert!(matches!(transcript.entry(0), Some((_, Entry::Text { text, .. })) if text == "Hello"));
}

#[test]
fn tool_result_updates_its_call_even_after_other_entries() {
    let mut transcript = Transcript::default();
    transcript.apply(call("t1"));
    transcript.apply(text(2, "working"));
    let result = AgentEvent::ToolResult {
        id: "t1".into(),
        output: "a\nb".into(),
        is_error: false,
    };
    assert_eq!(
        transcript.apply(result),
        Change {
            updated: Some(0),
            appended: 0
        }
    );
    assert!(
        matches!(transcript.entry(0), Some((_, Entry::Tool { result: Some(r), .. })) if r.output == "a\nb")
    );
}

#[test]
fn permissions_queue_and_are_answered_once() {
    let mut transcript = Transcript::default();
    transcript.apply(permission("p1"));
    transcript.apply(permission("p2"));
    assert_eq!(transcript.len(), 0, "permissions are not transcript rows");
    assert_eq!(
        transcript.pending_permission().map(|r| r.id.as_str()),
        Some("p1")
    );
    assert!(transcript.take_permission("p1").is_some());
    assert!(transcript.take_permission("p1").is_none());
    assert_eq!(
        transcript.pending_permission().map(|r| r.id.as_str()),
        Some("p2")
    );
}

#[test]
fn unanswered_permissions_expire_with_the_turn() {
    let mut transcript = Transcript::default();
    transcript.apply(permission("p1"));
    transcript.apply(AgentEvent::TurnEnded(TurnOutcome::Interrupted));
    assert!(transcript.pending_permission().is_none());
    transcript.apply(permission("p2"));
    transcript.apply(AgentEvent::Exited { error: None });
    assert!(transcript.pending_permission().is_none());
}

#[test]
fn a_reset_empties_the_transcript_but_the_turn_goes_on() {
    let mut transcript = Transcript::default();
    transcript.push_user("/clear".into());
    transcript.apply(call("t1"));
    transcript.apply(AgentEvent::ConversationReset);
    assert_eq!(transcript.len(), 0);
    assert_eq!(transcript.status(), Status::Running);
    transcript.push_user("next".into());
    assert_eq!(transcript.title(), Some("next"));
}

#[test]
fn a_stored_conversation_is_restored_idle() {
    let mut transcript = Transcript::default();
    transcript.push_user("gone".into());
    transcript.restore(vec![
        PastMessage::User("hi".into()),
        PastMessage::Agent(call("t1")),
        PastMessage::Agent(AgentEvent::ToolResult {
            id: "t1".into(),
            output: "ok".into(),
            is_error: false,
        }),
        PastMessage::Agent(text(0, "hello")),
    ]);
    assert_eq!(transcript.status(), Status::Idle);
    assert_eq!(transcript.title(), Some("hi"));
    assert_eq!(transcript.len(), 3);
    assert!(matches!(
        transcript.entry(1),
        Some((
            _,
            Entry::Tool {
                result: Some(_),
                ..
            }
        ))
    ));
}

#[test]
fn status_follows_the_turn_and_the_process() {
    let mut transcript = Transcript::default();
    assert_eq!(transcript.status(), Status::Idle);
    transcript.push_user("hi".into());
    assert_eq!(transcript.status(), Status::Running);
    transcript.apply(AgentEvent::TurnEnded(TurnOutcome::Interrupted));
    assert_eq!(transcript.status(), Status::Idle);
    transcript.apply(AgentEvent::Exited {
        error: Some("boom".into()),
    });
    assert_eq!(transcript.status(), Status::Ended);
    assert!(
        matches!(transcript.entry(2), Some((_, Entry::Notice { is_error: true, text })) if text.contains("boom"))
    );
}

#[test]
fn a_pasted_document_is_titled_by_its_first_line() {
    let mut transcript = Transcript::default();
    let _ = transcript.push_user("\n  # Handover notes\n\nRead AGENTS.md first.".into());
    assert_eq!(transcript.title(), Some("# Handover notes"));
}

#[test]
fn a_message_taken_back_before_any_reply_leaves_no_trace() {
    let mut transcript = Transcript::default();
    transcript.push_user("first".into());
    transcript.apply(text(0, "done"));
    transcript.apply(AgentEvent::TurnEnded(TurnOutcome::Completed));
    transcript.push_user("second".into());
    assert_eq!(transcript.sent().collect::<Vec<_>>(), ["second", "first"]);
    assert_eq!(transcript.withdraw(), Some((2, "second".to_owned())));
    transcript.apply(AgentEvent::TurnEnded(TurnOutcome::Interrupted));
    assert_eq!(transcript.len(), 2, "no interrupted line");
    assert_eq!(transcript.status(), Status::Idle);
}

#[test]
fn a_message_with_a_reply_is_not_taken_back() {
    let mut transcript = Transcript::default();
    transcript.push_user("go".into());
    transcript.apply(text(0, "on it"));
    assert_eq!(transcript.withdraw(), None);
    transcript.apply(AgentEvent::TurnEnded(TurnOutcome::Interrupted));
    assert!(matches!(
        transcript.entry(2),
        Some((_, Entry::Notice { .. }))
    ));
}
