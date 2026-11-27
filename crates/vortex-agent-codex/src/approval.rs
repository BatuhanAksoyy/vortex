//! Approvals: how a permission mode maps onto Codex's approval policy and
//! sandbox, the server's approval requests as permission cards, and the
//! user's decision as the response.

use serde_json::{Value, json};
use vortex_agent::{AgentEvent, Decision, PermissionMode, PermissionRequest, Remember, RuleScope};

use crate::{
    items,
    protocol::{CommandExecutionRequestApprovalParams, FileChangeRequestApprovalParams},
    rpc,
    translate::{Step, Translator, decode},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A command; the words of the rule an "always" answer adds.
    Command(Option<Vec<String>>),
    FileChange,
}

/// The approval policy and sandbox mode for `mode`. Codex has no mode of
/// its own for most of Vortex's: asking before edits asks for every command
/// not known to be safe and every change; editing automatically lets the
/// model write in the folder and asks only to step outside it.
pub fn policy(mode: PermissionMode) -> (&'static str, &'static str) {
    match mode {
        PermissionMode::AskBeforeEdits => ("untrusted", "workspace-write"),
        PermissionMode::EditAutomatically | PermissionMode::Auto => {
            ("on-request", "workspace-write")
        }
        PermissionMode::Plan => ("on-request", "read-only"),
        PermissionMode::DontAsk => ("never", "workspace-write"),
        PermissionMode::BypassPermissions => ("never", "danger-full-access"),
    }
}

/// The sandbox as `turn/start` takes it, for the same mode.
pub fn sandbox_policy(mode: PermissionMode) -> Value {
    match policy(mode).1 {
        "read-only" => json!({ "type": "readOnly" }),
        "danger-full-access" => json!({ "type": "dangerFullAccess" }),
        _ => json!({ "type": "workspaceWrite" }),
    }
}

impl Translator {
    pub(crate) fn on_request(&mut self, id: Value, method: &str, params: Value) -> Step {
        let asked = match method {
            "item/commandExecution/requestApproval" => {
                decode::<CommandExecutionRequestApprovalParams>(params).map(command_card)
            }
            "item/fileChange/requestApproval" => decode::<FileChangeRequestApprovalParams>(params)
                .map(|asked| self.change_card(&asked)),
            _ => None,
        };
        let Some((mut card, kind)) = asked else {
            return Step {
                out: vec![rpc::refusal(id, method)],
                ..Step::default()
            };
        };
        card.id = id.to_string();
        self.approvals.insert(card.id.clone(), (id, kind));
        Step {
            events: vec![AgentEvent::PermissionRequest(card)],
            ..Step::default()
        }
    }

    fn change_card(&self, asked: &FileChangeRequestApprovalParams) -> (PermissionRequest, Kind) {
        let files = self.changed_files.get(&asked.item_id);
        let file = files
            .and_then(|files| files.first())
            .cloned()
            .unwrap_or_default();
        let card = PermissionRequest {
            id: String::new(),
            tool: "Edit".to_owned(),
            input: json!({ "file_path": file }),
            description: asked.reason.clone(),
            remember: Some(Remember::AcceptEdits),
            questions: Vec::new(),
            subagent: None,
        };
        (card, Kind::FileChange)
    }

    /// The response to approval `request_id`; `None` for one already gone.
    pub(crate) fn decide(&mut self, request_id: &str, decision: &Decision) -> Option<Value> {
        let (id, kind) = self.approvals.remove(request_id)?;
        let decision = match (decision, kind) {
            (Decision::Allow | Decision::Answer(_), _) => json!("accept"),
            (Decision::AllowAlways(RuleScope::Session), _)
            | (Decision::AllowAlways(_), Kind::FileChange | Kind::Command(None)) => {
                json!("acceptForSession")
            }
            (Decision::AllowAlways(_), Kind::Command(Some(rule))) => {
                json!({ "acceptWithExecpolicyAmendment": { "execpolicy_amendment": rule } })
            }
            (Decision::Deny { .. }, _) => json!("decline"),
        };
        Some(rpc::response(id, json!({ "decision": decision })))
    }
}

fn command_card(asked: CommandExecutionRequestApprovalParams) -> (PermissionRequest, Kind) {
    let command = (asked.command_actions.as_deref())
        .and_then(|actions| match actions {
            [only] => Some(only),
            _ => None,
        })
        .and_then(items::action_command)
        .map(str::to_owned)
        .or(asked.command)
        .unwrap_or_default();
    let rule = asked.proposed_execpolicy_amendment;
    let card = PermissionRequest {
        id: String::new(),
        tool: "Bash".to_owned(),
        input: json!({ "command": command }),
        description: asked.reason,
        remember: rule
            .clone()
            .map(|words| Remember::Rules(vec![words.join(" ")])),
        questions: Vec::new(),
        subagent: None,
    };
    (card, Kind::Command(rule))
}
