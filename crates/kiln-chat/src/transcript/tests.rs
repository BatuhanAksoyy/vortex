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
        subagent: None,
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
        pictures: Vec::new(),
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
            pictures: Vec::new(),
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
    assert_eq!(transcript.withdraw(), Some((2, "second".into())));
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

fn subagent(change: kiln_agent::SubagentChange) -> AgentEvent {
    AgentEvent::Subagent(kiln_agent::SubagentEvent {
        call: "agent".into(),
        change,
    })
}

fn step(id: &str, name: &str) -> AgentEvent {
    AgentEvent::ToolCall(ToolCall {
        id: id.into(),
        parent: Some("agent".into()),
        name: name.into(),
        input: json!({}),
    })
}

#[test]
fn a_subagent_gathers_its_steps_and_report_in_its_card() {
    use kiln_agent::{SubagentChange, SubagentEnd};
    let mut transcript = Transcript::default();
    transcript.push_user("go".into());
    transcript.apply(AgentEvent::ToolCall(ToolCall {
        id: "agent".into(),
        parent: None,
        name: "Agent".into(),
        input: json!({}),
    }));
    transcript.apply(subagent(SubagentChange::Started {
        description: "Read the note".into(),
        kind: Some("general-purpose".into()),
    }));
    let launched = ToolResult {
        output: "Async agent launched".into(),
        pictures: Vec::new(),
        is_error: false,
    };
    transcript.apply(AgentEvent::ToolResult {
        id: "agent".into(),
        output: launched.output.clone(),
        pictures: Vec::new(),
        is_error: false,
    });
    assert_eq!(transcript.apply(step("read", "Read")).updated, Some(1));
    transcript.apply(AgentEvent::ToolResult {
        id: "read".into(),
        output: "lantern".into(),
        pictures: Vec::new(),
        is_error: false,
    });
    transcript.apply(subagent(SubagentChange::Model("haiku".into())));
    transcript.apply(subagent(SubagentChange::Text("lantern".into())));
    transcript.apply(AgentEvent::TurnEnded(TurnOutcome::Completed));
    transcript.apply(subagent(SubagentChange::Finished {
        end: SubagentEnd::Completed,
        summary: "lantern".into(),
        usage: None,
    }));
    assert_eq!(transcript.len(), 2, "the steps stay in the card");
    let agent = transcript.subagents().get(2).expect("the card");
    assert_eq!(agent.result.as_ref(), Some(&launched));
    assert_eq!(agent.model.as_deref(), Some("haiku"));
    assert_eq!(agent.end, Some(SubagentEnd::Completed));
    assert_eq!(agent.summary.as_deref(), Some("lantern"));
    let [Step::Tool { call, result }, Step::Text(text)] = agent.steps.as_slice() else {
        panic!("{:?}", agent.steps);
    };
    assert_eq!((call.name.as_str(), text.as_str()), ("Read", "lantern"));
    assert_eq!(result.as_ref().map(|r| r.output.as_str()), Some("lantern"));

    transcript.apply(AgentEvent::PromptTokens(kiln_agent::PromptTokens::default()));
    assert_eq!(
        transcript.status(),
        Status::Running,
        "the report starts a turn of its own"
    );
}

#[test]
fn steps_without_a_card_stay_on_the_timeline() {
    let mut transcript = Transcript::default();
    transcript.apply(call("agent"));
    transcript.apply(step("read", "Read"));
    assert_eq!(transcript.len(), 2);
}

#[test]
fn a_resumed_subagent_runs_again_in_its_card() {
    use kiln_agent::{SubagentChange, SubagentEnd};
    let mut transcript = Transcript::default();
    transcript.apply(AgentEvent::ToolCall(ToolCall {
        id: "agent".into(),
        parent: None,
        name: "Agent".into(),
        input: json!({}),
    }));
    transcript.apply(subagent(SubagentChange::Started {
        description: "Read the note".into(),
        kind: None,
    }));
    transcript.apply(subagent(SubagentChange::Finished {
        end: SubagentEnd::Completed,
        summary: "lantern".into(),
        usage: None,
    }));
    transcript.apply(subagent(SubagentChange::Resumed {
        message: "And the second word?".into(),
    }));
    let agent = transcript.subagents().get(1).expect("the card");
    assert_eq!(
        (agent.end, agent.summary.as_deref()),
        (None, None),
        "running"
    );
    assert_eq!(agent.steps, [Step::Message("And the second word?".into())]);
    transcript.apply(subagent(SubagentChange::Finished {
        end: SubagentEnd::Completed,
        summary: "moth".into(),
        usage: None,
    }));
    let agent = transcript.subagents().get(1).expect("the card");
    assert_eq!(agent.summary.as_deref(), Some("moth"));
}

#[test]
fn a_message_taken_back_from_behind_a_running_turn_leaves_it_interrupted() {
    let mut transcript = Transcript::default();
    transcript.push_user("count to 100".into());
    transcript.apply(text(0, "1 2 3"));
    transcript.push_user("and then stop".into());
    assert_eq!(transcript.withdraw(), Some((2, "and then stop".into())));
    transcript.apply(AgentEvent::TurnEnded(TurnOutcome::Interrupted));
    assert!(
        matches!(transcript.entry(2), Some((_, Entry::Notice { text, .. })) if text == "Interrupted"),
        "the turn that answered was interrupted"
    );
}

#[test]
fn a_take_back_without_an_end_does_not_hide_the_next_interruption() {
    let mut transcript = Transcript::default();
    transcript.push_user("first".into());
    transcript.withdraw();
    // The interrupted turn's end never came; the next turn starts anew.
    transcript.apply(AgentEvent::TurnEnded(TurnOutcome::Completed));
    transcript.push_user("second".into());
    transcript.apply(text(0, "working"));
    transcript.apply(AgentEvent::TurnEnded(TurnOutcome::Interrupted));
    assert!(matches!(
        transcript.entry(2),
        Some((_, Entry::Notice { .. }))
    ));
}

#[test]
fn an_interruption_stops_the_subagents_it_ran_in_the_foreground() {
    use kiln_agent::{SubagentChange, SubagentEnd};
    let mut transcript = Transcript::default();
    transcript.push_user("go".into());
    for (id, background) in [("agent", false), ("helper", true)] {
        transcript.apply(AgentEvent::ToolCall(ToolCall {
            id: id.into(),
            parent: None,
            name: "Agent".into(),
            input: json!({ "run_in_background": background }),
        }));
        transcript.apply(AgentEvent::Subagent(kiln_agent::SubagentEvent {
            call: id.into(),
            change: SubagentChange::Started {
                description: id.into(),
                kind: None,
            },
        }));
    }
    transcript.apply(AgentEvent::TurnEnded(TurnOutcome::Interrupted));
    let ends: Vec<_> = transcript
        .subagents()
        .iter()
        .map(|agent| agent.end)
        .collect();
    assert_eq!(ends, [Some(SubagentEnd::Stopped), None]);
    transcript.apply(AgentEvent::Exited { error: None });
    assert!(
        transcript
            .subagents()
            .iter()
            .all(|agent| agent.end == Some(SubagentEnd::Stopped)),
        "none runs on without the process"
    );
}
