//! Vortex's own MCP server, as a Codex thread takes it: in the thread's
//! configuration, added to the user's own servers.

use serde_json::{Map, Value, json};
use vortex_agent::ToolServer;

/// The thread's configuration for `server`. Its tools run without Codex
/// asking: Vortex asks the user itself about what matters, such as which
/// sites the browser may open. The user's own servers keep their rules.
pub fn config(server: &ToolServer) -> Value {
    let env: Map<String, Value> = (server.env.iter())
        .map(|(name, value)| (name.clone(), value.clone().into()))
        .collect();
    json!({"mcp_servers": {&server.name: {
        "command": server.command,
        "args": server.args,
        "env": env,
        "default_tools_approval_mode": "approve",
    }}})
}

/// What the model is told before the conversation: Vortex's tools, then what
/// another harness handed over.
pub fn instructions(server: Option<&ToolServer>, handoff: Option<&str>) -> Option<String> {
    let told = server.map(ToolServer::instructions);
    match (told, handoff) {
        (Some(told), Some(handoff)) => Some(format!("{told}\n\n{handoff}")),
        (told, handoff) => told.or_else(|| handoff.map(str::to_owned)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> ToolServer {
        ToolServer {
            name: "vortex".into(),
            command: "/Applications/Vortex".into(),
            args: vec!["mcp".into()],
            env: vec![("VORTEX_BRIDGE_TOKEN".into(), "abc".into())],
            commit_directives: vec!["Write in English.".into()],
        }
    }

    #[test]
    fn the_server_runs_its_tools_unasked() {
        let config = config(&server());
        let vortex = &config["mcp_servers"]["vortex"];
        assert_eq!(vortex["command"], "/Applications/Vortex");
        assert_eq!(vortex["args"], json!(["mcp"]));
        assert_eq!(vortex["env"]["VORTEX_BRIDGE_TOKEN"], "abc");
        assert_eq!(vortex["default_tools_approval_mode"], "approve");
    }

    #[test]
    fn the_tools_are_told_before_a_handoff() {
        let server = server();
        let told = instructions(Some(&server), Some("Earlier: hello")).unwrap();
        assert!(told.starts_with("This session runs in Vortex"), "{told}");
        assert!(told.contains("- Write in English."), "{told}");
        assert!(told.ends_with("\n\nEarlier: hello"), "{told}");
        assert_eq!(instructions(None, Some("x")).as_deref(), Some("x"));
        assert_eq!(instructions(None, None), None);
    }
}
