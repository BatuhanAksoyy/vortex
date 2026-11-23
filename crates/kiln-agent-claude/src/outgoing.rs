//! Messages Kiln writes to the CLI's stdin.

use kiln_agent::{Decision, RuleScope};

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

pub fn get_skills_dialog(request_id: &str) -> Value {
    control_request(request_id, &json!({ "subtype": "get_skills_dialog" }))
}

pub fn get_hooks_listing(request_id: &str) -> Value {
    control_request(request_id, &json!({ "subtype": "get_hooks_listing" }))
}

pub fn get_context_usage(request_id: &str) -> Value {
    control_request(request_id, &json!({ "subtype": "get_context_usage" }))
}

/// Starts a claude.ai sign-in; the reply carries the page to sign in on.
pub fn claude_authenticate(request_id: &str) -> Value {
    control_request(
        request_id,
        &json!({ "subtype": "claude_authenticate", "loginWithClaudeAi": true }),
    )
}

/// Answered once the sign-in in the browser has finished.
pub fn claude_oauth_wait_for_completion(request_id: &str) -> Value {
    control_request(
        request_id,
        &json!({ "subtype": "claude_oauth_wait_for_completion" }),
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
/// the tool with it. `remember` holds the CLI's own suggested permission
/// updates; sending them back is what makes it stop asking, kept where the
/// decision's scope says.
pub fn permission_response(
    request_id: &str,
    decision: &Decision,
    input: &Value,
    remember: Option<&Value>,
) -> Value {
    let response = match (decision, remember) {
        (Decision::AllowAlways(scope), Some(updates)) => json!({
            "behavior": "allow",
            "updatedInput": input,
            "updatedPermissions": scoped(updates, *scope),
        }),
        (Decision::Allow | Decision::AllowAlways(_), _) => {
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

/// `updates` with each added rule kept for `scope`. Other updates, such as
/// switching to accept edits, keep the CLI's own destination: they last for
/// the session only.
fn scoped(updates: &Value, scope: RuleScope) -> Value {
    let destination = match scope {
        RuleScope::Session => "session",
        RuleScope::Project => "localSettings",
        RuleScope::User => "userSettings",
    };
    let mut updates = updates.clone();
    let rules = updates
        .as_array_mut()
        .into_iter()
        .flatten()
        .filter(|update| update.get("type").and_then(Value::as_str) == Some("addRules"));
    for update in rules {
        update["destination"] = json!(destination);
    }
    updates
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
