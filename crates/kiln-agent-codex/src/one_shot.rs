//! One answer from a model, for Kiln's own small jobs such as a commit
//! message: a thread that is not stored, may change nothing and asks for
//! nothing.

use kiln_agent::OneShot;
use serde_json::{Value, json};

use crate::{client::Client, protocol::ThreadItem, protocol::TurnCompletedNotification};

pub async fn ask(program: std::path::PathBuf, ask: OneShot) -> anyhow::Result<String> {
    let mut client = Client::start(&program, ask.env, Some(&ask.cwd)).await?;
    let mut thread = json!({
        "cwd": ask.cwd.to_string_lossy(),
        "baseInstructions": ask.system,
        "ephemeral": true,
        "approvalPolicy": "never",
        "sandbox": "read-only",
    });
    // No model is the account's default one.
    if !ask.model.is_empty() {
        thread["model"] = json!(ask.model);
    }
    let started = client.request("thread/start", thread).await?;
    let Some(thread_id) = started.pointer("/thread/id").and_then(Value::as_str) else {
        anyhow::bail!("Codex started no thread");
    };
    let input = [json!({ "type": "text", "text": ask.prompt, "text_elements": [] })];
    let turn = json!({ "threadId": thread_id, "input": input });
    client.request("turn/start", turn).await?;
    let completed: TurnCompletedNotification =
        serde_json::from_value(client.until("turn/completed").await?)?;
    if let Some(error) = completed.turn.error {
        anyhow::bail!("{}", error.message);
    }
    let answer = completed
        .turn
        .items
        .iter()
        .rev()
        .find_map(|item| match item {
            ThreadItem::AgentMessage(message) => Some(message.text.trim().to_owned()),
            _ => None,
        });
    answer.ok_or_else(|| anyhow::anyhow!("The model gave no answer"))
}
