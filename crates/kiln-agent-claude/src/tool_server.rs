//! Kiln's own MCP server, as the CLI takes it.

use kiln_agent::ToolServer;
use serde_json::{Map, Value, json};

/// What the agent is told of Kiln's tools. Commits and pushes are Kiln's,
/// so the CLI's own ways to them are closed.
fn instructions(server: &str) -> String {
    format!(
        "This session runs in Kiln, an IDE whose tools are named mcp__{server}__*. Commit with \
         mcp__{server}__commit, never with `git commit`: Kiln holds the message to the \
         project's rules and the user may review it first. Do not push; Kiln pushes as the \
         user set it to."
    )
}

/// The CLI's settings while Kiln commits: no line crediting Claude, and its
/// own `git commit` and `git push` refused.
fn settings() -> Value {
    json!({
        "attribution": {"commit": "", "pr": ""},
        // What older CLIs read instead of `attribution`.
        "includeCoAuthoredBy": false,
        "permissions": {"deny": ["Bash(git commit:*)", "Bash(git push:*)"]},
    })
}

/// Added to the user's own servers, whose tools Kiln leaves to the CLI's
/// rules. Kiln's tools the CLI does not ask about: Kiln asks the user itself,
/// about what matters, such as which sites the browser may open.
pub(crate) fn args(server: &ToolServer) -> [String; 8] {
    let env: Map<String, Value> = server
        .env
        .iter()
        .map(|(name, value)| (name.clone(), value.clone().into()))
        .collect();
    let config = json!({"mcpServers": {&server.name: {
        "type": "stdio",
        "command": server.command,
        "args": server.args,
        "env": env,
    }}});
    [
        "--mcp-config".to_owned(),
        config.to_string(),
        "--allowedTools".to_owned(),
        format!("mcp__{}", server.name),
        "--append-system-prompt".to_owned(),
        instructions(&server.name),
        "--settings".to_owned(),
        settings().to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_server_is_named_and_its_tools_allowed() {
        let server = ToolServer {
            name: "kiln".into(),
            command: "/Applications/Kiln".into(),
            args: vec!["mcp".into()],
            env: vec![("KILN_BRIDGE_TOKEN".into(), "abc".into())],
        };
        let [
            flag,
            config,
            allow,
            allowed,
            prompt,
            told,
            flag_settings,
            settings,
        ] = args(&server);
        assert_eq!(
            (prompt.as_str(), flag_settings.as_str()),
            ("--append-system-prompt", "--settings")
        );
        assert!(told.contains("mcp__kiln__commit"), "{told}");
        let settings: Value = serde_json::from_str(&settings).unwrap();
        assert_eq!(settings["attribution"]["commit"], "");
        assert_eq!(settings["permissions"]["deny"][0], "Bash(git commit:*)");
        assert_eq!(
            (flag.as_str(), allow.as_str()),
            ("--mcp-config", "--allowedTools")
        );
        let config: Value = serde_json::from_str(&config).unwrap();
        let kiln = &config["mcpServers"]["kiln"];
        assert_eq!(kiln["command"], "/Applications/Kiln");
        assert_eq!(kiln["env"]["KILN_BRIDGE_TOKEN"], "abc");
        assert_eq!(allowed, "mcp__kiln");
    }
}
