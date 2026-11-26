//! Stands in for `codex app-server` in tests. Replays the fixture named by
//! `KILN_FAKE_CODEX_FIXTURE` as a server: what it recorded after each
//! message it read is printed when a message of the same kind arrives, the
//! next unused one for that kind, with the response carrying the id of the
//! request actually sent. Kiln may so send its requests in its own order.
//! A request no recording is left for gets an error, as one the server
//! refused; anything else unrecorded is ignored.

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead as _, Write as _};
use std::process::ExitCode;

use serde_json::Value;

/// What the server printed after one message it read.
struct Reply {
    /// The id the recorded request had, which its response carries.
    asked_as: Option<Value>,
    lines: Vec<Value>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            let _ = writeln!(std::io::stderr(), "fake-codex: {message}");
            ExitCode::from(3)
        }
    }
}

fn run() -> Result<(), String> {
    let path = std::env::var("KILN_FAKE_CODEX_FIXTURE").map_err(|e| format!("fixture: {e}"))?;
    let fixture = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
    let mut replies = script(&fixture)?;
    let mut stdout = std::io::stdout().lock();
    for line in std::io::stdin().lock().lines() {
        let message: Value =
            serde_json::from_str(&line.map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        let Some(reply) = replies
            .get_mut(&kind(&message))
            .and_then(VecDeque::pop_front)
        else {
            if message.get("method").is_some() && message.get("id").is_some() {
                let refused = serde_json::json!({
                    "id": message["id"], "error": { "code": -32600, "message": "not recorded" },
                });
                writeln!(stdout, "{refused}").map_err(|e| e.to_string())?;
                stdout.flush().map_err(|e| e.to_string())?;
            }
            continue;
        };
        for mut out in reply.lines {
            if out.get("method").is_none() && out.get("id") == reply.asked_as.as_ref() {
                out["id"] = message["id"].clone();
            }
            writeln!(stdout, "{out}").map_err(|e| e.to_string())?;
        }
        stdout.flush().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// The replies recorded for each kind of message read, in order.
fn script(fixture: &str) -> Result<HashMap<String, VecDeque<Reply>>, String> {
    let mut replies: HashMap<String, VecDeque<Reply>> = HashMap::new();
    let mut current: Option<(String, Reply)> = None;
    for record in fixture.lines() {
        let record: Value = serde_json::from_str(record).map_err(|e| e.to_string())?;
        let msg = record["msg"].clone();
        match record["dir"].as_str() {
            Some("in") => {
                if let Some((kind, reply)) = current.take() {
                    replies.entry(kind).or_default().push_back(reply);
                }
                let asked_as = msg.get("method").and(msg.get("id")).cloned();
                current = Some((
                    kind(&msg),
                    Reply {
                        asked_as,
                        lines: Vec::new(),
                    },
                ));
            }
            Some("out") => {
                if let Some((_, reply)) = &mut current {
                    reply.lines.push(msg);
                }
            }
            _ => {}
        }
    }
    if let Some((kind, reply)) = current {
        replies.entry(kind).or_default().push_back(reply);
    }
    Ok(replies)
}

/// A request or notification by its method; an answer to the server's own
/// request by its decision.
fn kind(message: &Value) -> String {
    match message.get("method").and_then(Value::as_str) {
        Some(method) => method.to_owned(),
        None => format!("answer {}", message["result"]["decision"]),
    }
}
