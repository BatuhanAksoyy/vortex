use std::path::PathBuf;

use anyhow::Context as _;
use async_process::{Child, ChildStdin, Command, Stdio};
use futures::{
    AsyncBufReadExt as _, AsyncReadExt as _, AsyncWriteExt as _, StreamExt as _, io::BufReader,
    stream,
};
use kiln_agent::{AgentBackend, AgentEvent, Session, SessionCommand, SessionSpec, SkillState};
use serde_json::Value;

use crate::{control, outgoing, skills::SkillEditor, translate::Translator};

const ARGS: &[&str] = &[
    "-p",
    "--input-format",
    "stream-json",
    "--output-format",
    "stream-json",
    "--verbose",
    "--include-partial-messages",
    "--permission-prompt-tool",
    "stdio",
];

/// Set by a parent Claude Code process. Inherited, they make the child behave
/// as a nested session of whatever terminal launched Kiln.
pub(crate) const PARENT_SESSION_VARS: &[&str] = &[
    "CLAUDECODE",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_CODE_EXECPATH",
    "CLAUDE_PID",
    "CLAUDE_EFFORT",
    "AI_AGENT",
];

/// Keeps a few kilobytes of stderr for the exit message.
const STDERR_TAIL_BYTES: usize = 4096;

pub struct ClaudeBackend {
    program: PathBuf,
}

impl ClaudeBackend {
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
        }
    }
}

impl AgentBackend for ClaudeBackend {
    fn spawn(&self, spec: SessionSpec) -> anyhow::Result<Session> {
        let mut command = Command::new(&self.program);
        command.args(ARGS);
        if let Some(model) = &spec.model {
            command.args(["--model", model]);
        }
        if let Some(mode) = spec.mode {
            command.args(["--permission-mode", control::mode_name(mode)]);
        }
        let editor = SkillEditor {
            program: self.program.clone(),
            cwd: spec.cwd.clone(),
            env: spec.env.clone(),
        };
        command
            .current_dir(&spec.cwd)
            .envs(spec.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        for var in PARENT_SESSION_VARS {
            command.env_remove(var);
        }
        let child = command
            .spawn()
            .with_context(|| format!("cannot start {}", self.program.display()))?;
        let (event_tx, events) = async_channel::unbounded();
        let (commands, command_rx) = async_channel::unbounded();
        Ok(Session {
            events,
            commands,
            driver: Box::pin(drive(child, editor, event_tx, command_rx)),
        })
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
    editor: SkillEditor,
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
    let mut translator = Translator::default();
    let session = async {
        let mut written = write(&mut stdin, &outgoing::initialize()).await;
        while written.is_ok() {
            match inputs.next().await {
                Some(Input::Line(line)) => send_all(&events, translator.on_line(&line)).await,
                Some(Input::Command(SessionCommand::SetSkillState { name, state })) => {
                    edit_skill(&editor, &translator, &events, name, state).await;
                    written =
                        forward(&mut stdin, &mut translator, SessionCommand::RefreshSkills).await;
                }
                Some(Input::Command(command)) => {
                    written = forward(&mut stdin, &mut translator, command).await;
                }
                Some(Input::OwnerGone) => {
                    let _ = child.kill();
                    break;
                }
                Some(Input::OutputClosed) | None => break,
            }
        }
        drop(stdin);
    };
    let stderr_tail = read_tail(stderr);
    let ((), stderr_tail) = futures::future::join(session, stderr_tail).await;
    let error = exit_error(&mut child, &stderr_tail).await;
    let _ = events.send(AgentEvent::Exited { error }).await;
}

/// Awaited inside the driver loop: the CLI's output waits in the pipe
/// meanwhile, and an edit takes a moment.
async fn edit_skill(
    editor: &SkillEditor,
    translator: &Translator,
    events: &async_channel::Sender<AgentEvent>,
    name: String,
    state: SkillState,
) {
    if let Err(error) = editor.run(&translator.skill_edit(&name, state)).await {
        let _ = events
            .send(AgentEvent::SkillStateFailed { name, error })
            .await;
    }
}

async fn forward(
    stdin: &mut ChildStdin,
    translator: &mut Translator,
    command: SessionCommand,
) -> std::io::Result<()> {
    match translator.on_command(command) {
        Some(message) => write(stdin, &message).await,
        None => Ok(()),
    }
}

async fn send_all(events: &async_channel::Sender<AgentEvent>, batch: Vec<AgentEvent>) {
    for event in batch {
        let _ = events.send(event).await;
    }
}

async fn write(stdin: &mut ChildStdin, message: &Value) -> std::io::Result<()> {
    let mut line = message.to_string();
    line.push('\n');
    stdin.write_all(line.as_bytes()).await?;
    stdin.flush().await
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
        Some(format!("claude exited with {status}"))
    } else {
        Some(stderr_tail.to_owned())
    }
}
