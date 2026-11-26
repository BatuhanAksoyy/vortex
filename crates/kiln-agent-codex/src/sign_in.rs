//! Signing an account in to `ChatGPT` through the app-server: it hands out the
//! sign-in page, the user finishes in the browser, and it saves the sign-in
//! in the `CODEX_HOME` its environment names. No terminal is involved.

use std::path::{Path, PathBuf};

use kiln_agent::{Account, SignIn};
use serde_json::{Value, json};

use crate::{client::Client, protocol::GetAccountResponse, report};

pub fn start(program: &Path, env: Vec<(String, String)>) -> SignIn {
    let (page, url) = async_channel::bounded(1);
    let program = program.to_path_buf();
    SignIn {
        url,
        done: Box::pin(run(program, env, page)),
    }
}

async fn run(
    program: PathBuf,
    env: Vec<(String, String)>,
    page: async_channel::Sender<String>,
) -> anyhow::Result<Account> {
    let mut client = Client::start(&program, env, None).await?;
    let started = client
        .request("account/login/start", json!({ "type": "chatgpt" }))
        .await?;
    let Some(address) = started.get("authUrl").and_then(Value::as_str) else {
        anyhow::bail!("Codex gave no sign-in page");
    };
    let _ = page.send(address.to_owned()).await;
    let completed = client.until("account/login/completed").await?;
    if completed.get("success").and_then(Value::as_bool) != Some(true) {
        let error = completed.get("error").and_then(Value::as_str);
        anyhow::bail!("{}", error.unwrap_or("The sign-in did not finish."));
    }
    let read = client
        .request("account/read", json!({ "refreshToken": false }))
        .await?;
    let read: GetAccountResponse = serde_json::from_value(read)?;
    read.account
        .as_ref()
        .and_then(report::account)
        .ok_or_else(|| anyhow::anyhow!("Codex signed in to no ChatGPT account"))
}
