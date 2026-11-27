//! The result line that ends a turn.

use std::time::Duration;

use vortex_agent::{AgentEvent, TurnOutcome};

use crate::{usage, wire};

/// What `result` reports. The one of a recap Vortex asked for, `of_recap`,
/// ends no turn.
pub fn events(result: wire::TurnResult, of_recap: bool) -> Vec<AgentEvent> {
    let mut events: Vec<AgentEvent> = usage::spent(&result.model_usage)
        .map(AgentEvent::SpentTokens)
        .into_iter()
        .collect();
    if of_recap {
        return events;
    }
    let (turns, took) = (result.num_turns, result.duration_ms);
    let outcome = outcome(result);
    if outcome == TurnOutcome::Completed && turns > 0 {
        events.push(AgentEvent::TurnTook {
            duration: Duration::from_millis(took),
            ended_at: None,
        });
    }
    events.push(AgentEvent::TurnEnded(outcome));
    events
}

fn outcome(result: wire::TurnResult) -> TurnOutcome {
    if result.subtype == "success" && !result.is_error {
        return TurnOutcome::Completed;
    }
    if result
        .terminal_reason
        .as_deref()
        .is_some_and(|reason| reason.starts_with("aborted"))
    {
        return TurnOutcome::Interrupted;
    }
    TurnOutcome::Failed(result.result.unwrap_or(result.subtype))
}
