//! The CLI's settings writers, which the VS Code extension runs for its
//! settings screens: one JSON edit on stdin, the result on stdout. The CLI
//! keeps the files' format and checks the edit, so Kiln never writes them.

use std::path::PathBuf;

use async_process::{Command, Stdio};
use futures::AsyncWriteExt as _;
use serde_json::Value;

use crate::session::PARENT_SESSION_VARS;

/// Also set by a parent Claude Code process; the extension clears it for
/// the edit too.
const TRACE_VAR: &str = "TRACEPARENT";

/// Runs one of the CLI's settings writers for one edit.
pub struct CliEditor {
    pub program: PathBuf,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
}

impl CliEditor {
    /// Runs `claude <subcommand> --json` with `edit` on stdin. The error is
    /// the CLI's own message, ready to show.
    pub async fn run(&self, subcommand: &str, edit: &Value) -> Result<(), String> {
        let mut command = Command::new(&self.program);
        command
            .args([subcommand, "--json"])
            .current_dir(&self.cwd)
            .envs(self.env.iter().cloned())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        for var in PARENT_SESSION_VARS.iter().chain(&[TRACE_VAR]) {
            command.env_remove(var);
        }
        let mut child = command
            .spawn()
            .map_err(|error| format!("cannot start {}: {error}", self.program.display()))?;
        if let Some(mut stdin) = child.stdin.take() {
            let written = stdin.write_all(edit.to_string().as_bytes()).await;
            drop(stdin);
            written.map_err(|error| format!("cannot send the edit: {error}"))?;
        }
        let output = child
            .output()
            .await
            .map_err(|error| format!("cannot read the edit's result: {error}"))?;
        if output.status.success() {
            return Ok(());
        }
        let said = if output.stderr.iter().all(u8::is_ascii_whitespace) {
            output.stdout
        } else {
            output.stderr
        };
        let message = String::from_utf8_lossy(&said).trim().to_owned();
        Err(if message.is_empty() {
            format!("claude {subcommand} exited with {}", output.status)
        } else {
            message
        })
    }
}
