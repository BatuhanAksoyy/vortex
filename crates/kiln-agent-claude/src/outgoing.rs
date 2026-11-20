//! Messages Kiln writes to the CLI's stdin.

use kiln_agent::Decision;

use crate::ask;
use serde_json::{Value, json};

pub const INITIALIZE_ID: &str = "initialize";

pub fn initialize() -> Value {
    control_request(
        INITIALIZE_ID,
        &json!({ "subtype": "initialize", "hooks": null }),
    )
}

pub fn set_model(request_id: &str, model: &str) -> Value {
    control_request(
        request_id,
        &json!({ "subtype": "set_model", "model": model }),
    )
}

pub fn set_permission_mode(request_id: &str, mode: &str) -> Value {
    control_request(
        request_id,
        &json!({ "subtype": "set_permission_mode", "mode": mode }),
    )
}

pub fn get_usage(request_id: &str) -> Value {
    control_request(request_id, &json!({ "subtype": "get_usage" }))
}

pub fn get_context_usage(request_id: &str) -> Value {
    control_request(request_id, &json!({ "subtype": "get_context_usage" }))
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
/// the tool with it. `remember` holds the CLI's own suggested permission
/// updates; sending them back is what makes it stop asking.
pub fn permission_response(
    request_id: &str,
    decision: &Decision,
    input: &Value,
    remember: Option<&Value>,
) -> Value {
    let response = match (decision, remember) {
        (Decision::AllowAlways, Some(updates)) => json!({
            "behavior": "allow",
            "updatedInput": input,
            "updatedPermissions": updates,
        }),
        (Decision::Allow | Decision::AllowAlways, _) => {
            json!({ "behavior": "allow", "updatedInput": input })
        }
        (Decision::Deny { feedback }, _) => {
            json!({ "behavior": "deny", "message": deny_message(feedback.as_deref()) })
        }
        (Decision::Answer(answers), _) => json!({
            "behavior": "allow",
            "updatedInput": ask::answered_input(input, answers),
        }),
    };
    json!({
        "type": "control_response",
        "response": { "subtype": "success", "request_id": request_id, "response": response },
    })
}

fn deny_message(feedback: Option<&str>) -> String {
    match feedback.map(str::trim).filter(|text| !text.is_empty()) {
        Some(text) => format!("The user denied this tool call and said: {text}"),
        None => "The user denied this tool call.".to_owned(),
    }
}

fn control_request(request_id: &str, request: &Value) -> Value {
    json!({ "type": "control_request", "request_id": request_id, "request": request })
}
