//! A short-lived app-server for Kiln's own jobs outside a chat: signing
//! in, reading stored threads, asking a model once. Requests are answered
//! in turn; server requests are turned down.

use std::path::Path;

use anyhow::{Context as _, bail};
use async_process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use futures::{AsyncBufReadExt as _, AsyncWriteExt as _, io::BufReader, io::Lines};
use serde_json::{Value, json};

use crate::rpc::{self, Incoming};

pub const ARGS: &[&str] = &["app-server"];

/// The command that starts the app-server with `env`, without what a
/// parent Claude Code session would pass on.
pub fn command(program: &Path, env: Vec<(String, String)>) -> Command {
    let mut command = Command::new(program);
    command
        .args(ARGS)
        .envs(env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .kill_on_drop(true);
    for var in kiln_agent::PARENT_SESSION_VARS {
        command.env_remove(var);
    }
    command
}

pub fn initialize_params() -> Value {
    json!({
        "clientInfo": { "name": "kiln", "title": "Kiln", "version": env!("CARGO_PKG_VERSION") },
    })
}

pub struct Client {
    /// Kept so the process ends with the client.
    _child: Child,
    stdin: ChildStdin,
    lines: Lines<BufReader<ChildStdout>>,
    next_id: i64,
}

impl Client {
    /// Starts an app-server in `cwd` and introduces Kiln to it.
    pub async fn start(
        program: &Path,
        env: Vec<(String, String)>,
        cwd: Option<&Path>,
    ) -> anyhow::Result<Self> {
        let mut command = command(program, env);
        command.stderr(Stdio::null());
        if let Some(cwd) = cwd {
            command.current_dir(cwd);
        }
        let mut child = command
            .spawn()
            .with_context(|| format!("cannot start {}", program.display()))?;
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            bail!("cannot talk to {}", program.display());
        };
        let mut client = Self {
            _child: child,
            stdin,
            lines: BufReader::new(stdout).lines(),
            next_id: 0,
        };
        client.request("initialize", initialize_params()).await?;
        client.write(&rpc::notification("initialized")).await?;
        Ok(client)
    }

    /// Sends a request and waits for its result.
    pub async fn request(&mut self, method: &str, params: Value) -> anyhow::Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        self.write(&rpc::request(id, method, params)).await?;
        loop {
            if let Incoming::Response {
                id: answered,
                result,
            } = self.read().await?
                && answered == id
            {
                return result
                    .map_err(anyhow::Error::msg)
                    .with_context(|| format!("Codex refused {method}"));
            }
        }
    }

    /// Waits for notification `method` and returns its parameters.
    pub async fn until(&mut self, method: &str) -> anyhow::Result<Value> {
        loop {
            if let Incoming::Notification {
                method: came,
                params,
            } = self.read().await?
                && came == method
            {
                return Ok(params);
            }
        }
    }

    async fn read(&mut self) -> anyhow::Result<Incoming> {
        use futures::StreamExt as _;
        loop {
            let Some(line) = self.lines.next().await else {
                bail!("Codex ended before it answered");
            };
            match Incoming::parse(&line?) {
                Some(Incoming::Request { id, method, .. }) => {
                    self.write(&rpc::refusal(id, &method)).await?;
                }
                Some(message) => return Ok(message),
                None => {}
            }
        }
    }

    async fn write(&mut self, message: &Value) -> anyhow::Result<()> {
        write(&mut self.stdin, message)
            .await
            .context("cannot write to Codex")
    }
}

pub async fn write(stdin: &mut ChildStdin, message: &Value) -> std::io::Result<()> {
    let mut line = message.to_string();
    line.push('\n');
    stdin.write_all(line.as_bytes()).await?;
    stdin.flush().await
}
