//! Replies to the control requests Kiln sends (initialize, model and mode
//! changes, context usage), and the CLI's names for permission modes.

use kiln_agent::{
    Account, AgentEvent, ContextCategory, ContextKind, ContextUsage, ModelOption, PermissionMode,
    SessionInfo, SlashCommand,
};
use serde::Deserialize;
use serde_json::Value;

use crate::wire::WireCommand;

/// What a sent request asked for, so its reply can be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Asked {
    Initialize,
    Model,
    Mode,
    Context,
    Usage,
    /// Read by the translator, which keeps the skills' other names.
    Skills,
}

pub fn mode_name(mode: PermissionMode) -> &'static str {
    match mode {
        // The start-up flag accepts "manual" but not "default"; mode changes
        // accept both.
        PermissionMode::AskBeforeEdits => "manual",
        PermissionMode::EditAutomatically => "acceptEdits",
        PermissionMode::Plan => "plan",
        PermissionMode::Auto => "auto",
        PermissionMode::DontAsk => "dontAsk",
        PermissionMode::BypassPermissions => "bypassPermissions",
    }
}

pub fn parse_mode(name: &str) -> Option<PermissionMode> {
    Some(match name {
        "default" | "manual" => PermissionMode::AskBeforeEdits,
        "acceptEdits" => PermissionMode::EditAutomatically,
        "plan" => PermissionMode::Plan,
        "auto" => PermissionMode::Auto,
        "dontAsk" => PermissionMode::DontAsk,
        "bypassPermissions" => PermissionMode::BypassPermissions,
        _ => return None,
    })
}

#[derive(Deserialize)]
struct Initialized {
    #[serde(default)]
    models: Vec<WireModel>,
    #[serde(default)]
    current_permission_mode: String,
    account: Option<WireAccount>,
    #[serde(default)]
    commands: Vec<WireCommand>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireAccount {
    #[serde(default)]
    email: String,
    #[serde(default)]
    organization: String,
    #[serde(default)]
    subscription_type: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireModel {
    value: String,
    #[serde(default)]
    resolved_model: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    description: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireContext {
    total_tokens: u64,
    max_tokens: u64,
    percentage: Option<u64>,
    #[serde(default)]
    categories: Vec<WireCategory>,
}

#[derive(Deserialize)]
struct WireCategory {
    name: String,
    tokens: u64,
    #[serde(default)]
    kind: String,
}

/// The event a successful reply means, if any.
pub fn on_success(asked: Asked, response: Value) -> Option<AgentEvent> {
    match asked {
        Asked::Initialize => {
            let init: Initialized = decode(response)?;
            Some(AgentEvent::Ready(SessionInfo {
                models: init.models.into_iter().map(model_option).collect(),
                mode: parse_mode(&init.current_permission_mode).unwrap_or_default(),
                account: init.account.map(|account| Account {
                    email: account.email,
                    organization: account.organization,
                    plan: account.subscription_type,
                }),
                commands: slash_commands(init.commands),
            }))
        }
        Asked::Model => {
            let model = response.get("model")?.as_str()?.to_owned();
            Some(AgentEvent::ModelChanged(model))
        }
        // The CLI also announces the new mode in a status line, which covers
        // changes Kiln did not ask for; that line is the one translated.
        Asked::Mode | Asked::Skills => None,
        Asked::Usage => crate::usage::from_reply(&response).map(AgentEvent::Usage),
        Asked::Context => {
            let context: WireContext = decode(response)?;
            Some(AgentEvent::ContextUsage(ContextUsage {
                used_tokens: context.total_tokens,
                max_tokens: context.max_tokens,
                percent: context.percentage,
                categories: context.categories.into_iter().map(category).collect(),
            }))
        }
    }
}

/// The commands a message can run. Internal ones (`__name`) and ones whose
/// listed name holds a space (MCP prompts, listed as `server:prompt (MCP)`)
/// cannot be typed as `/name` and are left out.
pub fn slash_commands(commands: Vec<WireCommand>) -> Vec<SlashCommand> {
    commands
        .into_iter()
        .filter(|command| {
            !command.name.is_empty()
                && !command.name.starts_with("__")
                && !command.name.contains(char::is_whitespace)
        })
        .map(|command| SlashCommand {
            name: command.name,
            description: command.description,
            argument_hint: command.argument_hint,
            aliases: command.aliases,
        })
        .collect()
}

fn category(category: WireCategory) -> ContextCategory {
    let kind = match category.kind.as_str() {
        "deferred" => ContextKind::Deferred,
        "buffer" => ContextKind::Buffer,
        "free" => ContextKind::Free,
        _ => ContextKind::Used,
    };
    ContextCategory {
        name: category.name,
        tokens: category.tokens,
        kind,
    }
}

fn model_option(model: WireModel) -> ModelOption {
    let name = if model.display_name.is_empty() {
        model.value.clone()
    } else {
        model.display_name
    };
    ModelOption {
        resolved: if model.resolved_model.is_empty() {
            model.value.clone()
        } else {
            model.resolved_model
        },
        id: model.value,
        name,
        description: model.description,
    }
}

fn decode<T: for<'de> Deserialize<'de>>(response: Value) -> Option<T> {
    serde_json::from_value(response)
        .map_err(|error| log::warn!("unreadable control reply: {error}"))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mode_round_trips_through_its_cli_name() {
        for mode in [
            PermissionMode::AskBeforeEdits,
            PermissionMode::EditAutomatically,
            PermissionMode::Plan,
            PermissionMode::Auto,
            PermissionMode::DontAsk,
            PermissionMode::BypassPermissions,
        ] {
            assert_eq!(parse_mode(mode_name(mode)), Some(mode));
        }
        assert_eq!(parse_mode("manual"), Some(PermissionMode::AskBeforeEdits));
        assert_eq!(parse_mode("unknown"), None);
    }

    #[test]
    fn commands_that_cannot_be_typed_are_left_out() {
        let listed = |name: &str| WireCommand {
            name: name.into(),
            description: String::new(),
            argument_hint: String::new(),
            aliases: Vec::new(),
        };
        let names: Vec<String> = slash_commands(vec![
            listed("compact"),
            listed("__remote-workflow"),
            listed("kicad:via_usage (MCP)"),
            listed("antigravity:delegate"),
        ])
        .into_iter()
        .map(|command| command.name)
        .collect();
        assert_eq!(names, ["compact", "antigravity:delegate"]);
    }
}
