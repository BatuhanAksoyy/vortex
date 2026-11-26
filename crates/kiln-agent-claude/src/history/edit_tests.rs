use kiln_agent::{AgentEvent, PastMessage};

use super::messages;

/// A conversation stored by `claude -p` (capture.py edit): one Edit changes
/// two lines next to each other.
const STORED: &str = include_str!("../../tests/fixtures/edit-session.jsonl");

#[test]
fn a_stored_edit_keeps_its_patch() {
    let messages = messages(STORED);
    let patches: Vec<_> = messages
        .iter()
        .filter_map(|message| match message {
            PastMessage::Agent(AgentEvent::ToolResult { patch, .. }) if !patch.is_empty() => {
                Some(patch)
            }
            _ => None,
        })
        .collect();
    assert_eq!(patches.len(), 1, "{messages:#?}");
    assert_eq!(patches[0][0].old_start, 1);
    assert!(patches[0][0].lines.contains(&"+FOUR".to_owned()));
}
