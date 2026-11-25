//! Kiln's own MCP server, as the CLI takes it.

use kiln_agent::ToolServer;
use serde_json::{Map, Value, json};

/// Added to the user's own servers, whose tools Kiln leaves to the CLI's
/// rules. Kiln's tools the CLI does not ask about: Kiln asks the user itself,
/// about what matters, such as which sites the browser may open.
pub(crate) fn args(server: &ToolServer) -> [String; 4] {
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
        let [flag, config, allow, allowed] = args(&server);
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
