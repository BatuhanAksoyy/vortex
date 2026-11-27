//! Vortex's own MCP server, as the CLI takes it.

use serde_json::{Map, Value, json};
use vortex_agent::ToolServer;

/// The CLI's settings while Vortex commits: no line crediting Claude, and its
/// own `git commit` and `git push` refused. A deny rule holds in every
/// permission mode, bypass too, so it is given only then.
fn settings() -> Value {
    json!({
        "attribution": {"commit": "", "pr": ""},
        // What older CLIs read instead of `attribution`.
        "includeCoAuthoredBy": false,
        "permissions": {"deny": ["Bash(git commit:*)", "Bash(git push:*)"]},
    })
}

/// Added to the user's own servers, whose tools Vortex leaves to the CLI's
/// rules. Vortex's tools the CLI does not ask about: Vortex asks the user itself,
/// about what matters, such as which sites the browser may open.
/// The CLI's arguments for `server`; `handoff` goes after its instructions,
/// as the CLI takes one appended prompt.
pub(crate) fn args(server: &ToolServer, handoff: Option<&str>) -> Vec<String> {
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
    let mut args = vec![
        "--mcp-config".to_owned(),
        config.to_string(),
        "--allowedTools".to_owned(),
        format!("mcp__{}", server.name),
        "--append-system-prompt".to_owned(),
        handoff.map_or_else(
            || server.instructions(),
            |handoff| format!("{}\n\n{handoff}", server.instructions()),
        ),
    ];
    if server.commits {
        args.extend(["--settings".to_owned(), settings().to_string()]);
    } else {
        args.extend([
            "--disallowedTools".to_owned(),
            format!("mcp__{}__commit", server.name),
        ]);
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_server_is_named_and_its_tools_allowed() {
        let server = ToolServer {
            name: "vortex".into(),
            command: "/Applications/Vortex".into(),
            args: vec!["mcp".into()],
            env: vec![("VORTEX_BRIDGE_TOKEN".into(), "abc".into())],
            commits: true,
            commit_directives: vec!["Write in English.".into()],
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
        ] = <[String; 8]>::try_from(args(&server, None)).unwrap();
        assert_eq!(
            (prompt.as_str(), flag_settings.as_str()),
            ("--append-system-prompt", "--settings")
        );
        assert!(told.contains("mcp__vortex__commit"), "{told}");
        assert!(told.ends_with("- Write in English."), "{told}");
        let settings: Value = serde_json::from_str(&settings).unwrap();
        assert_eq!(settings["attribution"]["commit"], "");
        assert_eq!(settings["permissions"]["deny"][0], "Bash(git commit:*)");
        assert_eq!(
            (flag.as_str(), allow.as_str()),
            ("--mcp-config", "--allowedTools")
        );
        let config: Value = serde_json::from_str(&config).unwrap();
        let vortex = &config["mcpServers"]["vortex"];
        assert_eq!(vortex["command"], "/Applications/Vortex");
        assert_eq!(vortex["env"]["VORTEX_BRIDGE_TOKEN"], "abc");
        assert_eq!(allowed, "mcp__vortex");

        let own_git = ToolServer {
            commits: false,
            ..server
        };
        let args = args(&own_git, None);
        assert!(
            !args.contains(&"--settings".to_owned()),
            "git is not refused"
        );
        assert_eq!(
            args[args.len() - 2..],
            ["--disallowedTools", "mcp__vortex__commit"]
        );
        assert!(!args[5].contains("git commit"), "{}", args[5]);
    }
}
