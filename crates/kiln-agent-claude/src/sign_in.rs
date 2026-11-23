//! Signing an account in through the control protocol, as the VS Code
//! extension does: the CLI hands out a sign-in page, the user finishes in the
//! browser, and the CLI saves the sign-in to the credential store its
//! environment names. No terminal is involved.

use std::path::Path;

use anyhow::{Context as _, bail};
use async_process::{ChildStdin, ChildStdout, Command, Stdio};
use futures::{AsyncBufReadExt as _, StreamExt as _, io::BufReader};
use kiln_agent::{Account, SignIn};
use serde_json::Value;

use crate::{
    control, outgoing,
    session::{ARGS, PARENT_SESSION_VARS, write},
};

const AUTHENTICATE_ID: &str = "kiln-sign-in-1";
const COMPLETION_ID: &str = "kiln-sign-in-2";

pub fn start(program: &Path, env: Vec<(String, String)>) -> anyhow::Result<SignIn> {
    let mut command = Command::new(program);
    command
        .args(ARGS)
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
    let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        bail!("cannot talk to {}", program.display());
    };
    let (page, url) = async_channel::bounded(1);
    let done = async move {
        let signed_in = run(stdin, stdout, page).await;
        // Ends the CLI, and with it the sign-in, also when `done` is dropped
        // before this point.
        drop(child);
        signed_in
    };
    Ok(SignIn {
        url,
        done: Box::pin(done),
    })
}

async fn run(
    mut stdin: ChildStdin,
    stdout: ChildStdout,
    page: async_channel::Sender<String>,
) -> anyhow::Result<Account> {
    write(&mut stdin, &outgoing::initialize()).await?;
    write(&mut stdin, &outgoing::claude_authenticate(AUTHENTICATE_ID)).await?;
    let mut lines = BufReader::new(stdout).lines();
    while let Some(Ok(line)) = lines.next().await {
        match reply(&line) {
            Some(Reply::Ok(id, body)) if id == AUTHENTICATE_ID => {
                let url = body["automaticUrl"]
                    .as_str()
                    .context("claude gave no sign-in page")?;
                let _ = page.send(url.to_owned()).await;
                let wait = outgoing::claude_oauth_wait_for_completion(COMPLETION_ID);
                write(&mut stdin, &wait).await?;
            }
            Some(Reply::Ok(id, body)) if id == COMPLETION_ID => {
                return control::signed_in(&body).context("the sign-in named no account");
            }
            Some(Reply::Failed(id, error)) if id == AUTHENTICATE_ID || id == COMPLETION_ID => {
                bail!("{error}");
            }
            _ => {}
        }
    }
    bail!("claude exited before the sign-in finished")
}

enum Reply {
    Ok(String, Value),
    Failed(String, String),
}

/// A control response on `line`, by its request id.
fn reply(line: &str) -> Option<Reply> {
    let message: Value = serde_json::from_str(line).ok()?;
    if message["type"] != "control_response" {
        return None;
    }
    let response = &message["response"];
    let id = response["request_id"].as_str()?.to_owned();
    if response["subtype"] == "success" {
        Some(Reply::Ok(id, response["response"].clone()))
    } else {
        let error = response["error"].as_str().unwrap_or("the sign-in failed");
        Some(Reply::Failed(id, error.to_owned()))
    }
}
