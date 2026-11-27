//! Codex's reasoning and subagents, replayed from captured sessions.

// Inside a `cfg(test)` module so helpers may `unwrap`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod reasoning {
    use kiln_agent::{AgentEvent, SessionCommand};
    use kiln_agent_codex::{Start, Translator};
    use serde_json::json;

    use super::common::{records, text, translate};

    #[test]
    fn a_thought_is_timed_and_counted_once_its_tokens_are_reported() {
        let events = translate("thinking");
        let thought = events.iter().find_map(|e| match e {
            AgentEvent::ThinkingDelta { block, .. } => Some(block.message_id.clone()),
            _ => None,
        });
        let thought = thought.unwrap();
        let words: String = events
            .iter()
            .filter_map(|e| match e {
                AgentEvent::ThinkingDelta { block, text } if block.message_id == thought => {
                    Some(text.as_str())
                }
                _ => None,
            })
            .collect();
        assert!(words.starts_with("**"), "a bold summary line: {words}");
        let took = events.iter().find_map(|e| match e {
            AgentEvent::ThinkingTime {
                message_id,
                duration,
            } if *message_id == thought => Some(*duration),
            _ => None,
        });
        assert!(took.is_some_and(|took| took.as_millis() > 0));
        let tokens = events.iter().find_map(|e| match e {
            AgentEvent::ThoughtTokens { message_id, tokens } if *message_id == thought => {
                Some(*tokens)
            }
            _ => None,
        });
        assert_eq!(tokens, Some(124), "the reasoning tokens of its request");
    }

    #[test]
    fn reasoning_items_in_a_row_join_one_thought() {
        let mut translator = Translator::new(Start::default());
        let item = |id: &str, at: i64, done: bool| {
            let method = if done {
                "item/completed"
            } else {
                "item/started"
            };
            let time = if done { "completedAtMs" } else { "startedAtMs" };
            let item =
                json!({ "type": "reasoning", "id": id, "summary": [format!("**Step {id}**")] });
            json!({ "method": method, "params": { "threadId": "t", "turnId": "u", "item": item, time: at }})
            .to_string()
        };
        let mut events = Vec::new();
        for line in [
            item("a", 1000, false),
            item("a", 3000, true),
            item("b", 3000, false),
            item("b", 4500, true),
        ] {
            events.extend(translator.on_line(&line).events);
        }
        let blocks: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                AgentEvent::ThinkingDelta { block, text } => {
                    Some((block.message_id.as_str(), text.as_str()))
                }
                _ => None,
            })
            .collect();
        assert_eq!(blocks, [("a", "**Step a**"), ("a", "\n\n**Step b**")]);
        let last_time = events.iter().rev().find_map(|e| match e {
            AgentEvent::ThinkingTime { duration, .. } => Some(duration.as_millis()),
            _ => None,
        });
        assert_eq!(last_time, Some(3500), "from the first item's start");
    }

    #[test]
    fn a_subagent_runs_inside_its_call_and_not_as_the_main_turn() {
        use kiln_agent::{SubagentChange, SubagentEnd};
        let events = translate("subagent");
        let ended = events
            .iter()
            .filter(|e| matches!(e, AgentEvent::TurnEnded(_)));
        assert_eq!(ended.count(), 1, "the subagent's turn is not the main one");
        let call = events.iter().find_map(|e| match e {
            AgentEvent::ToolCall(call) if call.name == "Agent" => Some(call),
            _ => None,
        });
        let call = call.unwrap();
        assert_eq!(call.parent, None);
        let changes: Vec<&SubagentChange> = events
            .iter()
            .filter_map(|e| match e {
                AgentEvent::Subagent(event) if event.call == call.id => Some(&event.change),
                _ => None,
            })
            .collect();
        assert!(matches!(
            changes[0],
            SubagentChange::Started { description, .. } if description == "heron word"
        ));
        assert!(
            changes
                .iter()
                .any(|c| matches!(c, SubagentChange::Text(t) if t.contains("heron")))
        );
        assert!(matches!(
            changes.last().unwrap(),
            SubagentChange::Finished { end: SubagentEnd::Completed, summary, .. } if summary.contains("heron")
        ));
        let reported = events
            .iter()
            .any(|e| matches!(e, AgentEvent::ToolResult { id, .. } if *id == call.id));
        assert!(reported, "its call ends with its report");
        let blocks: std::collections::HashSet<_> = events
            .iter()
            .filter_map(|e| match e {
                AgentEvent::TextDelta { block, .. } => Some(block.message_id.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            blocks.len(),
            1,
            "only the main thread's answer is the chat's text"
        );
        assert!(text(&events).to_lowercase().contains("heron"));
    }

    #[test]
    fn stopping_a_subagent_interrupts_its_own_turn() {
        let mut translator = Translator::new(Start::default());
        let mut call = None;
        for (dir, msg) in records("subagent") {
            if dir == "in" {
                translator.on_sent(&msg.to_string());
                continue;
            }
            let events = translator.on_line(&msg.to_string()).events;
            call = call.or_else(|| {
                events.iter().find_map(|e| match e {
                    AgentEvent::ToolCall(c) if c.name == "Agent" => Some(c.id.clone()),
                    _ => None,
                })
            });
            // Once the subagent's turn runs.
            if msg["method"] == "item/started" && msg["params"]["item"]["type"] == "agentMessage" {
                break;
            }
        }
        let out = translator
            .on_command(SessionCommand::StopSubagent(call.unwrap()))
            .out;
        assert_eq!(out[0]["method"], "turn/interrupt");
        assert!(
            out[0]["params"]["threadId"]
                .as_str()
                .unwrap()
                .ends_with("c488")
        );
    }
}
