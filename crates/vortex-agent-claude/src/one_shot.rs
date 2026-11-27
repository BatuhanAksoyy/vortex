//! One answer from `claude -p`, for Vortex's own small jobs. The model runs
//! with no tools, no MCP servers, no settings (so no hooks run in the
//! user's project) and its session is not kept.

use async_process::{Command, Stdio};
use futures::AsyncWriteExt as _;
use serde::Deserialize;
use vortex_agent::OneShot;

use crate::session::PARENT_SESSION_VARS;

const ARGS: &[&str] = &[
    "-p",
    "--output-format",
    "json",
    "--tools",
    "",
    "--strict-mcp-config",
    "--no-session-persistence",
    "--disable-slash-commands",
    "--setting-sources",
    "",
];

/// The one line `--output-format json` prints.
#[derive(Debug, Deserialize)]
pub struct Answer {
    #[serde(default)]
    pub is_error: bool,
    #[serde(default)]
    pub result: String,
}

pub async fn ask(program: std::path::PathBuf, ask: OneShot) -> anyhow::Result<String> {
    let mut command = Command::new(program);
    command
        .args(ARGS)
        .args(["--system-prompt", &ask.system])
        .current_dir(&ask.cwd)
        .envs(ask.env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    // No model is the CLI's own default.
    if !ask.model.is_empty() {
        command.args(["--model", &ask.model]);
    }
    for var in PARENT_SESSION_VARS {
        command.env_remove(var);
    }
    let mut child = command.spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(ask.prompt.as_bytes()).await?;
    }
    let output = child.output().await?;
    let printed = String::from_utf8_lossy(&output.stdout);
    let answer = answer(&printed).map_err(|error| {
        let said = String::from_utf8_lossy(&output.stderr);
        anyhow::anyhow!("{error}: {}", said.trim())
    })?;
    Ok(answer)
}

/// The model's answer in what the CLI printed, or why there is none.
pub fn answer(printed: &str) -> Result<String, String> {
    let answer: Answer =
        serde_json::from_str(printed.trim()).map_err(|_| "the agent gave no answer".to_owned())?;
    if answer.is_error {
        return Err(answer.result);
    }
    Ok(answer.result.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::answer;

    /// What `claude -p --output-format json` printed for a commit message.
    const CAPTURED: &str = include_str!("../tests/fixtures/one-shot.json");

    #[test]
    fn the_answer_is_read_from_what_the_cli_printed() {
        assert_eq!(
            answer(CAPTURED).as_deref(),
            Ok("Add \"more\" line to a.txt")
        );
        for needle in ["/home/", "/Users/", "/tmp/", "@"] {
            assert!(!CAPTURED.contains(needle), "{needle:?}");
        }
        let failed =
            r#"{"type":"result","subtype":"success","is_error":true,"result":"Not logged in"}"#;
        assert_eq!(answer(failed), Err("Not logged in".to_owned()));
        assert!(answer("").is_err());
    }
}
