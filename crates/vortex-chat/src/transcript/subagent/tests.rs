use serde_json::json;
use vortex_agent::{AgentEvent, PermissionRequest, TurnOutcome};

use super::*;

fn call(id: &str, parent: Option<&str>, background: bool) -> AgentEvent {
    AgentEvent::ToolCall(ToolCall {
        id: id.into(),
        parent: parent.map(Into::into),
        name: "Agent".into(),
        input: json!({ "run_in_background": background }),
    })
}

fn change(call: &str, change: SubagentChange) -> AgentEvent {
    let call = call.into();
    AgentEvent::Subagent(SubagentEvent { call, change })
}

fn start(transcript: &mut Transcript, id: &str, parent: Option<&str>, background: bool) {
    transcript.apply(call(id, parent, background));
    let started = SubagentChange::Started {
        description: id.into(),
        kind: None,
    };
    transcript.apply(change(id, started));
}

fn agent<'a>(transcript: &'a Transcript, name: &str) -> &'a Subagent {
    let mut all = transcript.subagents().iter();
    all.find(|agent| agent.description == name).unwrap()
}

fn names(agents: &[&Subagent]) -> Vec<String> {
    agents
        .iter()
        .map(|agent| agent.description.clone())
        .collect()
}

/// `outer` starts `a` and `b`; `a` starts `deep`.
fn family() -> Transcript {
    let mut transcript = Transcript::default();
    transcript.push_user("go".into());
    start(&mut transcript, "outer", None, false);
    start(&mut transcript, "a", Some("outer"), false);
    start(&mut transcript, "b", Some("outer"), false);
    start(&mut transcript, "deep", Some("a"), false);
    transcript
}

#[test]
fn a_subagents_subagent_is_a_step_of_the_one_that_started_it() {
    let transcript = family();
    assert_eq!(transcript.len(), 2, "one card on the timeline");
    let (outer, a, b) = (
        agent(&transcript, "outer"),
        agent(&transcript, "a"),
        agent(&transcript, "b"),
    );
    assert_eq!(outer.steps, [Step::Subagent(a.id), Step::Subagent(b.id)]);
    let agents = transcript.subagents();
    assert_eq!(names(&agents.below(outer.id)), ["a", "b", "deep"]);
    let deep = agent(&transcript, "deep");
    assert_eq!(names(&agents.path(deep.id)), ["outer", "a", "deep"]);
    assert_eq!((deep.row, agents.len(), agents.running()), (1, 4, 4));
}

#[test]
fn each_keeps_its_own_words_calls_and_end() {
    let mut transcript = family();
    transcript.apply(change("a", SubagentChange::Text("from a".into())));
    transcript.apply(change("a", SubagentChange::Model("haiku".into())));
    transcript.apply(change("b", SubagentChange::Text("from b".into())));
    let end = SubagentChange::Finished {
        end: SubagentEnd::Completed,
        summary: "b's report".into(),
        usage: None,
    };
    assert_eq!(transcript.apply(change("b", end)).updated, Some(1));
    transcript.apply(AgentEvent::ToolResult {
        id: "b".into(),
        output: "b's report".into(),
        pictures: Vec::new(),
        is_error: false,
        patch: Vec::new(),
    });
    let (a, b) = (agent(&transcript, "a"), agent(&transcript, "b"));
    assert!(a.steps.contains(&Step::Text("from a".into())));
    assert_eq!((a.model.as_deref(), a.end), (Some("haiku"), None));
    assert_eq!(b.steps, [Step::Text("from b".into())]);
    assert_eq!(b.end, Some(SubagentEnd::Completed));
    assert!(b.result.is_some());
    assert_eq!(agent(&transcript, "outer").end, None);
}

#[test]
fn a_refused_start_fails_only_its_own_card() {
    let mut transcript = family();
    transcript.apply(AgentEvent::ToolResult {
        id: "deep".into(),
        output: "depth limit".into(),
        pictures: Vec::new(),
        is_error: true,
        patch: Vec::new(),
    });
    assert_eq!(agent(&transcript, "deep").end, Some(SubagentEnd::Failed));
    assert_eq!(agent(&transcript, "a").end, None);
}

#[test]
fn an_interruption_leaves_what_a_background_subagent_waits_for_running() {
    let mut transcript = Transcript::default();
    transcript.push_user("go".into());
    start(&mut transcript, "near", None, false);
    start(&mut transcript, "near-child", Some("near"), false);
    start(&mut transcript, "near-helper", Some("near"), true);
    start(&mut transcript, "far", None, true);
    start(&mut transcript, "far-child", Some("far"), false);
    transcript.apply(AgentEvent::TurnEnded(TurnOutcome::Interrupted));
    let running: Vec<_> = transcript
        .subagents()
        .iter()
        .filter(|agent| agent.end.is_none())
        .collect();
    assert_eq!(names(&running), ["near-helper", "far", "far-child"]);
}

#[test]
fn an_ended_subagent_takes_its_permission_request_along() {
    let mut transcript = Transcript::default();
    start(&mut transcript, "a", None, true);
    transcript.apply(AgentEvent::PermissionRequest(PermissionRequest {
        id: "ask".into(),
        tool: "Bash".into(),
        input: serde_json::json!({}),
        description: None,
        remember: None,
        questions: Vec::new(),
        subagent: Some("a".into()),
    }));
    assert_eq!(transcript.waits_for(agent(&transcript, "a")), Some("Bash"));
    transcript.apply(AgentEvent::TurnEnded(TurnOutcome::Completed));
    assert!(
        transcript.pending_permission().is_some(),
        "it waits on in the background"
    );
    let finished = SubagentChange::Finished {
        end: SubagentEnd::Stopped,
        summary: String::new(),
        usage: None,
    };
    transcript.apply(change("a", finished));
    assert_eq!(transcript.pending_permission(), None);
}
