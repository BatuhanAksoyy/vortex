//! Messages Kiln writes to the CLI's stdin.

use kiln_agent::Decision;
use serde_json::{Value, json};

pub fn initialize() -> Value {
    control_request(
        "initialize",
        &json!({ "subtype": "initialize", "hooks": null }),
    )
}

pub fn interrupt(request_id: &str) -> Value {
    control_request(request_id, &json!({ "subtype": "interrupt" }))
}

pub fn user_message(text: &str) -> Value {
    json!({
        "type": "user",
        "message": { "role": "user", "content": text },
        "parent_tool_use_id": null,
        "session_id": "",
    })
}

/// `input` is echoed back on allow: the CLI requires `updatedInput` and runs
/// the tool with it.
pub fn permission_response(request_id: &str, decision: Decision, input: &Value) -> Value {
    let response = match decision {
        Decision::Allow => json!({ "behavior": "allow", "updatedInput": input }),
        Decision::Deny => {
            json!({ "behavior": "deny", "message": "The user denied this tool call." })
        }
    };
    json!({
        "type": "control_response",
        "response": { "subtype": "success", "request_id": request_id, "response": response },
    })
}

fn control_request(request_id: &str, request: &Value) -> Value {
    json!({ "type": "control_request", "request_id": request_id, "request": request })
}
