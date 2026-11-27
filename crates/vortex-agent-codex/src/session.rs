use std::path::{Path, PathBuf};

use anyhow::Context as _;
use async_process::{Child, ChildStdin, Stdio};
use futures::{AsyncBufReadExt as _, AsyncReadExt as _, StreamExt as _, io::BufReader, stream};
use vortex_agent::{
    AgentBackend, AgentEvent, Features, OneShot, PastMessage, PastSession, Pending, Session,
    SessionCommand, SessionSpec, SignIn,
};

use crate::{
    client::{self, write},
    history, one_shot, sign_in,
    translate::{Start, Step, Translator},
};

/// Keeps a few kilobytes of stderr for the exit message.
const STDERR_TAIL_BYTES: usize = 4096;

pub struct CodexBackend {
    program: PathBuf,
}

impl CodexBackend {
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
        }
    }
}

impl AgentBackend for CodexBackend {
    fn name(&self) -> &'static str {
        "codex"
    }

    fn spawn(&self, spec: SessionSpec) -> anyhow::Result<Session> {
        let mut command = client::command(&self.program, spec.env);
        command.current_dir(&spec.cwd).stderr(Stdio::piped());
        let child = command
            .spawn()
            .with_context(|| format!("cannot start {}", self.program.display()))?;
        let start = Start {
            cwd: spec.cwd.to_string_lossy().into_owned(),
            model: spec.model,
            mode: spec.mode.unwrap_or_default(),
            effort: spec.effort,
            resume: spec.resume,
            instructions: spec.handoff,
            tools: spec.tools,
        };
        let pid = child.id();
        let (event_tx, events) = async_channel::unbounded();
        let (commands, command_rx) = async_channel::unbounded();
        Ok(Session {
            pid: Some(pid),
            events,
            commands,
            driver: Box::pin(drive(child, Translator::new(start), event_tx, command_rx)),
        })
    }

    fn sign_in(&self, env: Vec<(String, String)>) -> anyhow::Result<SignIn> {
        Ok(sign_in::start(&self.program, env))
    }

    fn past_sessions(&self, cwd: &Path) -> Pending<Vec<PastSession>> {
        let (program, cwd) = (self.program.clone(), cwd.to_path_buf());
        Box::pin(async move { history::list(&program, &cwd).await })
    }

    fn past_messages(&self, _cwd: &Path, session_id: &str) -> Pending<Vec<PastMessage>> {
        let (program, session_id) = (self.program.clone(), session_id.to_owned());
        Box::pin(async move { history::read(&program, &session_id).await })
    }

    fn features(&self) -> Features {
        // Vortex does not read Codex's skills and hooks yet, and its prompt
        // cache is not one Vortex can time.
        Features {
            skills_and_hooks: false,
            prompt_cache: false,
        }
    }

    fn ask_once(&self, ask: OneShot) -> Pending<String> {
        Box::pin(one_shot::ask(self.program.clone(), ask))
    }
}

enum Input {
    Line(String),
    Command(SessionCommand),
    /// stdout closed: the process is exiting.
    OutputClosed,
    /// The session owner dropped its command sender.
    OwnerGone,
}

async fn drive(
    mut child: Child,
    mut translator: Translator,
    events: async_channel::Sender<AgentEvent>,
    commands: async_channel::Receiver<SessionCommand>,
) {
    let (Some(mut stdin), Some(stdout), Some(stderr)) =
        (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        return;
    };
    let lines = BufReader::new(stdout)
        .lines()
        .filter_map(|line| async move { line.ok() })
        .map(Input::Line)
        .chain(stream::iter([Input::OutputClosed]));
    let commands = commands
        .map(Input::Command)
        .chain(stream::iter([Input::OwnerGone]));
    let mut inputs = std::pin::pin!(stream::select(lines, commands));
    let mut refused = None;
    // Ended by Vortex itself, so its end is no error.
    let mut left = false;
    let session = async {
        let mut written = write(&mut stdin, &translator.begin()).await;
        while written.is_ok() {
            let step = match inputs.next().await {
                Some(Input::Line(line)) => translator.on_line(&line),
                Some(Input::Command(command)) => translator.on_command(command),
                Some(Input::OwnerGone) => {
                    left = true;
                    let _ = child.kill();
                    break;
                }
                Some(Input::OutputClosed) | None => break,
            };
            if step.exit.is_some() {
                refused = step.exit;
                let _ = child.kill();
                break;
            }
            written = apply(&mut stdin, &events, step).await;
        }
        drop(stdin);
    };
    let stderr_tail = read_tail(stderr);
    let ((), stderr_tail) = futures::future::join(session, stderr_tail).await;
    let error = match refused {
        Some(error) => Some(error),
        None if left => None,
        None => exit_error(&mut child, &stderr_tail).await,
    };
    let _ = events.send(AgentEvent::Exited { error }).await;
}

async fn apply(
    stdin: &mut ChildStdin,
    events: &async_channel::Sender<AgentEvent>,
    step: Step,
) -> std::io::Result<()> {
    for event in step.events {
        let _ = events.send(event).await;
    }
    for message in &step.out {
        write(stdin, message).await?;
    }
    Ok(())
}

async fn read_tail(stderr: async_process::ChildStderr) -> String {
    let mut text = String::new();
    let _ = BufReader::new(stderr).read_to_string(&mut text).await;
    let start = text.len().saturating_sub(STDERR_TAIL_BYTES);
    let start = (start..text.len())
        .find(|&ix| text.is_char_boundary(ix))
        .unwrap_or(text.len());
    text[start..].trim().to_owned()
}

/// `None` for a clean exit.
async fn exit_error(child: &mut Child, stderr_tail: &str) -> Option<String> {
    let status = match child.status().await {
        Ok(status) => status,
        Err(error) => return Some(format!("cannot read exit status: {error}")),
    };
    if status.success() {
        return None;
    }
    if stderr_tail.is_empty() {
        Some(format!("codex exited with {status}"))
    } else {
        Some(stderr_tail.to_owned())
    }
}
