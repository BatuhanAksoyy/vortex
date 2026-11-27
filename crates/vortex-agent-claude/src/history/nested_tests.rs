//! A stored conversation whose subagent started two of its own (capture.py
//! nested).

use vortex_agent::{AgentEvent, PastMessage, SubagentChange, SubagentEnd, SubagentEvent};

use super::tests::{ends, reopened};

const SESSION: &str = include_str!("../../tests/fixtures/nested-session.jsonl");
const AGENTS: [(&str, &str, &str); 3] = [
    (
        "a58309589a654b699",
        include_str!("../../tests/fixtures/nested-agent-a58309589a654b699.meta.json"),
        include_str!("../../tests/fixtures/nested-agent-a58309589a654b699.jsonl"),
    ),
    (
        "ac65d165714d7c0bd",
        include_str!("../../tests/fixtures/nested-agent-ac65d165714d7c0bd.meta.json"),
        include_str!("../../tests/fixtures/nested-agent-ac65d165714d7c0bd.jsonl"),
    ),
    (
        "a1208db2aa688a53e",
        include_str!("../../tests/fixtures/nested-agent-a1208db2aa688a53e.meta.json"),
        include_str!("../../tests/fixtures/nested-agent-a1208db2aa688a53e.jsonl"),
    ),
];
const OUTER: &str = "toolu_01XaQaWqtD3G5dxcMReYs24o";
const INNER: [&str; 2] = [
    "toolu_01KqxE1pQ3UxVVW21XKW1h4H",
    "toolu_01Bqs2FP2Qf5XwWzARQhuHYP",
];

#[test]
fn a_stored_subagents_own_subagents_come_back_with_their_steps() {
    let messages = reopened(SESSION, &AGENTS);
    let position = |wanted: &dyn Fn(&AgentEvent) -> bool| {
        messages
            .iter()
            .position(|message| matches!(message, PastMessage::Agent(event) if wanted(event)))
            .unwrap()
    };
    let started = |call: &str| {
        position(&|event| {
            matches!(event, AgentEvent::Subagent(SubagentEvent {
                call: of,
                change: SubagentChange::Started { .. },
            }) if of == call)
        })
    };
    for inner in INNER {
        let called = position(&|event| {
            matches!(event, AgentEvent::ToolCall(call)
                if call.id == inner && call.parent.as_deref() == Some(OUTER))
        });
        let read = position(&|event| {
            matches!(event, AgentEvent::ToolCall(call)
                if call.name == "Read" && call.parent.as_deref() == Some(inner))
        });
        assert!(started(OUTER) < called && called < started(inner) && started(inner) < read);
    }
    let mut ended: Vec<_> = ends(&messages)
        .into_iter()
        .map(|(call, end, _)| (call, end))
        .collect();
    ended.sort_by_key(|(call, _)| [OUTER, INNER[0], INNER[1]].iter().position(|it| it == call));
    let done = |call: &str| (call.to_owned(), SubagentEnd::Completed);
    assert_eq!(ended, [done(OUTER), done(INNER[0]), done(INNER[1])]);
}

#[test]
fn nested_fixtures_hold_no_personal_data() {
    let texts = AGENTS.iter().flat_map(|(_, meta, text)| [*meta, *text]);
    for text in texts.chain([SESSION]) {
        for needle in ["/home/", "/Users/", "/tmp/", "@"] {
            assert!(!text.contains(needle), "contains {needle:?}");
        }
    }
}
