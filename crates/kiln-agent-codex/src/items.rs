//! A thread item's start and end, as tool calls and their results.
//!
//! Commands show as `Bash` calls and file changes as `Edit` / `Write` calls,
//! the names the chat already draws cards for, with the input those cards
//! read (`command`, `file_path`).

use kiln_agent::{AgentEvent, ToolCall};
use serde_json::{Value, json};

use crate::{
    patch,
    protocol::{
        CommandExecutionStatus, FileUpdateChange, PatchApplyStatus, PatchChangeKind, ThreadItem,
        ThreadItemCommandExecution, ThreadItemFileChange, ThreadItemMcpToolCall,
    },
};

/// The calls an item starts; empty for an item that is no tool call.
pub fn started(item: &ThreadItem) -> Vec<AgentEvent> {
    let call = |id: &str, name: &str, input: Value| {
        AgentEvent::ToolCall(ToolCall {
            id: id.to_owned(),
            parent: None,
            name: name.to_owned(),
            input,
        })
    };
    match item {
        ThreadItem::CommandExecution(run) => {
            vec![call(&run.id, "Bash", json!({ "command": command(run) }))]
        }
        ThreadItem::FileChange(change) => (change.changes.iter().enumerate())
            .map(|(index, file)| {
                let name = match file.kind {
                    PatchChangeKind::Add => "Write",
                    _ => "Edit",
                };
                call(
                    &change_id(change, index),
                    name,
                    json!({ "file_path": file.path }),
                )
            })
            .collect(),
        ThreadItem::McpToolCall(tool) => {
            let name = format!("mcp__{}__{}", tool.server, tool.tool);
            vec![call(&tool.id, &name, tool.arguments.clone())]
        }
        ThreadItem::WebSearch(search) => {
            vec![call(
                &search.id,
                "WebSearch",
                json!({ "query": search.query }),
            )]
        }
        _ => Vec::new(),
    }
}

/// The results an item ends with; empty for an item that is no tool call.
pub fn completed(item: &ThreadItem) -> Vec<AgentEvent> {
    match item {
        ThreadItem::CommandExecution(run) => vec![command_result(run)],
        ThreadItem::FileChange(change) => file_results(change),
        ThreadItem::McpToolCall(tool) => vec![tool_result(tool)],
        ThreadItem::WebSearch(search) => vec![result(&search.id, String::new(), false)],
        _ => Vec::new(),
    }
}

/// The command the model wrote, without the login shell Codex wraps it in.
pub fn command(run: &ThreadItemCommandExecution) -> String {
    match run.command_actions.as_slice() {
        [only] => action_command(only).unwrap_or(&run.command).to_owned(),
        _ => run.command.clone(),
    }
}

pub(crate) fn action_command(action: &crate::protocol::CommandAction) -> Option<&str> {
    use crate::protocol::CommandAction as Action;
    Some(match action {
        Action::Read(read) => &read.command,
        Action::ListFiles(list) => &list.command,
        Action::Search(search) => &search.command,
        Action::Unknown(unknown) => &unknown.command,
        Action::Unrecognized => return None,
    })
}

/// One call per file: the item's own id for its first file.
pub fn change_id(change: &ThreadItemFileChange, index: usize) -> String {
    if index == 0 {
        change.id.clone()
    } else {
        format!("{}#{index}", change.id)
    }
}

fn result(id: &str, output: String, is_error: bool) -> AgentEvent {
    AgentEvent::ToolResult {
        id: id.to_owned(),
        output,
        pictures: Vec::new(),
        is_error,
        patch: Vec::new(),
    }
}

fn command_result(run: &ThreadItemCommandExecution) -> AgentEvent {
    let output = run.aggregated_output.clone().unwrap_or_default();
    match run.status {
        CommandExecutionStatus::Declined => {
            result(&run.id, "The command was declined.".to_owned(), true)
        }
        CommandExecutionStatus::Failed => result(&run.id, output, true),
        _ => result(&run.id, output, run.exit_code.is_some_and(|code| code != 0)),
    }
}

fn file_results(change: &ThreadItemFileChange) -> Vec<AgentEvent> {
    let failed = !matches!(change.status, PatchApplyStatus::Completed);
    let output = match change.status {
        PatchApplyStatus::Declined => "The change was declined.",
        PatchApplyStatus::Failed => "The change could not be applied.",
        _ => "",
    };
    (change.changes.iter().enumerate())
        .map(|(index, file)| AgentEvent::ToolResult {
            id: change_id(change, index),
            output: output.to_owned(),
            pictures: Vec::new(),
            is_error: failed,
            patch: file_patch(file),
        })
        .collect()
}

/// A new file's text shows in the card already; only a change gets hunks.
fn file_patch(file: &FileUpdateChange) -> Vec<kiln_agent::Hunk> {
    match file.kind {
        PatchChangeKind::Add => Vec::new(),
        _ => patch::hunks(&file.diff),
    }
}

/// An MCP result's text, and its pictures, such as a browser screenshot.
fn tool_result(tool: &ThreadItemMcpToolCall) -> AgentEvent {
    if let Some(error) = &tool.error {
        return result(&tool.id, error.message.clone(), true);
    }
    let blocks: Vec<&Value> = (tool.result.iter())
        .flat_map(|result| &result.content)
        .collect();
    let texts = (blocks.iter()).filter_map(|block| block.get("text").and_then(Value::as_str));
    AgentEvent::ToolResult {
        id: tool.id.clone(),
        output: texts.collect::<Vec<_>>().join("\n"),
        pictures: blocks.iter().filter_map(|block| picture(block)).collect(),
        is_error: false,
        patch: Vec::new(),
    }
}

/// An MCP image block's picture; `None` for another block or bad base64.
fn picture(block: &Value) -> Option<kiln_agent::Picture> {
    use base64::Engine as _;
    if block.get("type")? != "image" {
        return None;
    }
    let data = base64::engine::general_purpose::STANDARD
        .decode(block.get("data")?.as_str()?)
        .ok()?;
    Some(kiln_agent::Picture {
        media_type: block.get("mimeType")?.as_str()?.to_owned(),
        data: data.into(),
    })
}
