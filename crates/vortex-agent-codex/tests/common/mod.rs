//! The captured app-server sessions (`tests/fixtures/*.jsonl`), and the
//! events the translator makes of them.

use serde_json::Value;
use vortex_agent::AgentEvent;
use vortex_agent_codex::{Start, Translator};

pub fn records(name: &str) -> Vec<(String, Value)> {
    let path = format!("{}/tests/fixtures/{name}.jsonl", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(path).unwrap();
    text.lines()
        .map(|line| {
            let record: Value = serde_json::from_str(line).unwrap();
            (
                record["dir"].as_str().unwrap().to_owned(),
                record["msg"].clone(),
            )
        })
        .collect()
}

/// Every event the server's side of the capture leads to, in order.
pub fn translate(name: &str) -> Vec<AgentEvent> {
    let mut translator = Translator::new(Start::default());
    let mut events = Vec::new();
    for (dir, msg) in records(name) {
        match dir.as_str() {
            "in" => translator.on_sent(&msg.to_string()),
            "out" => events.extend(translator.on_line(&msg.to_string()).events),
            _ => {}
        }
    }
    events
}

pub fn text(events: &[AgentEvent]) -> String {
    let texts = events.iter().filter_map(|event| match event {
        AgentEvent::TextDelta { text, .. } => Some(text.as_str()),
        _ => None,
    });
    texts.collect()
}
