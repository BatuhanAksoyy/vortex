//! Plan usage asked of a CLI started for it alone, for an added account.
//! Within a minute of a fetch the CLI answers from the snapshot it keeps in
//! its configuration, tagged with the account that configuration names; in
//! the shared `~/.claude.json` that is the last-signed-in account, whatever
//! credential store the session uses. Run with a configuration directory of
//! the account's own, the snapshot is that account's, so the answer comes at
//! once and is right, and the shared configuration is left alone.

use std::path::PathBuf;

use anyhow::{Context as _, bail};
use async_process::{Command, Stdio};
use futures::{AsyncBufReadExt as _, StreamExt as _, io::BufReader};
use kiln_agent::Usage;
use serde_json::Value;

use crate::{
    outgoing,
    session::{PARENT_SESSION_VARS, write},
};

/// No tools, MCP servers or settings files: nothing the user configured
/// runs for a question about usage.
const ARGS: &[&str] = &[
    "-p",
    "--input-format",
    "stream-json",
    "--output-format",
    "stream-json",
    "--verbose",
    "--tools",
    "",
    "--strict-mcp-config",
    "--no-session-persistence",
    "--setting-sources",
    "",
];
const USAGE_ID: &str = "kiln-usage";

pub async fn ask(program: PathBuf, env: Vec<(String, String)>) -> anyhow::Result<Usage> {
    let mut command = Command::new(&program);
    command
        .args(ARGS)
        .current_dir(std::env::temp_dir())
        .envs(env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    for var in PARENT_SESSION_VARS {
        command.env_remove(var);
    }
    let mut child = command
        .spawn()
        .with_context(|| format!("cannot start {}", program.display()))?;
    let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        bail!("cannot talk to {}", program.display());
    };
    write(&mut stdin, &outgoing::initialize()).await?;
    let mut lines = BufReader::new(stdout).lines();
    while let Some(Ok(line)) = lines.next().await {
        match reply(&line) {
            Some((id, Ok(_))) if id == outgoing::INITIALIZE_ID => {
                write(&mut stdin, &outgoing::get_plan_usage(USAGE_ID)).await?;
            }
            Some((id, Ok(body))) if id == USAGE_ID => {
                return crate::usage::from_reply(&body).with_context(|| no_usage(&body));
            }
            Some((_, Err(error))) => bail!("{error}"),
            _ => {}
        }
    }
    bail!("claude exited before saying the usage")
}

/// Why a reply holds no plan usage. With limits that apply to the account,
/// the usage endpoint refused the ask: it answers an account only so often
/// (HTTP 429), and the CLI then does not ask it again for a while.
fn no_usage(body: &Value) -> String {
    if body["rate_limits_available"] == true {
        return "Claude refused to say the account's usage for now".to_owned();
    }
    format!(
        "the account has no plan usage (subscription: {})",
        body["subscription_type"]
    )
}

/// A control response on `line`: its request id, and its body or error.
fn reply(line: &str) -> Option<(String, Result<Value, String>)> {
    let message: Value = serde_json::from_str(line).ok()?;
    if message["type"] != "control_response" {
        return None;
    }
    let response = &message["response"];
    let id = response["request_id"].as_str()?.to_owned();
    if response["subtype"] == "success" {
        return Some((id, Ok(response["response"].clone())));
    }
    let error = response["error"]
        .as_str()
        .unwrap_or("claude did not say the usage");
    Some((id, Err(error.to_owned())))
}

#[cfg(test)]
mod tests {
    use super::reply;

    #[test]
    fn reads_a_control_reply_by_its_id() {
        let ok = r#"{"type":"control_response","response":{"subtype":"success","request_id":"kiln-usage","response":{"rate_limits":null}}}"#;
        let (id, body) = reply(ok).unwrap();
        assert_eq!(id, "kiln-usage");
        assert_eq!(body.unwrap()["rate_limits"], serde_json::Value::Null);
        let failed = r#"{"type":"control_response","response":{"subtype":"error","request_id":"kiln-usage","error":"nope"}}"#;
        assert_eq!(reply(failed).unwrap().1, Err("nope".to_owned()));
        assert!(reply(r#"{"type":"system"}"#).is_none());
    }
}
