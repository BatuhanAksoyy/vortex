//! Vortex's end of the bridge: the socket, and who may use it.

use std::{
    collections::HashSet,
    fmt::Write as _,
    io::{BufRead as _, BufReader, Read as _, Write as _},
    os::unix::{
        fs::PermissionsExt as _,
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use anyhow::Context as _;
use async_channel::{Receiver, Sender};
use serde_json::Value;
use vortex_agent::ToolServer;

use crate::{
    SERVER_NAME, SOCKET_VAR, TOKEN_VAR,
    mcp::{self, Incoming},
    tools::{Reply, Request},
};

/// Longer, a socket's path does not fit the address it is bound to.
const MAX_SOCKET_PATH: usize = 100;
const TOKEN_BYTES: usize = 16;

/// A tool call from the chat the token was given to.
pub struct Call {
    pub token: String,
    pub request: Request,
    pub reply: Sender<Reply>,
}

pub struct Bridge {
    socket: PathBuf,
    program: PathBuf,
    tokens: Arc<Mutex<HashSet<String>>>,
}

impl Bridge {
    /// Listens on a socket in `dir` that only this user can open; agents
    /// reach it by starting `program mcp`.
    pub fn start(dir: &Path, program: PathBuf) -> anyhow::Result<(Self, Receiver<Call>)> {
        static STARTED: AtomicUsize = AtomicUsize::new(0);
        let name = format!(
            "bridge-{}-{}.sock",
            std::process::id(),
            STARTED.fetch_add(1, Ordering::Relaxed)
        );
        let mut socket = dir.join(&name);
        if socket.as_os_str().len() > MAX_SOCKET_PATH {
            socket = std::env::temp_dir().join(name);
        }
        if let Some(parent) = socket.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Left by a Vortex that ended without cleaning up.
        let _ = std::fs::remove_file(&socket);
        let listener = UnixListener::bind(&socket)
            .with_context(|| format!("cannot listen at {}", socket.display()))?;
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
        let tokens = Arc::<Mutex<HashSet<String>>>::default();
        let (calls, received) = async_channel::unbounded();
        let known = tokens.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (known, calls) = (known.clone(), calls.clone());
                std::thread::spawn(move || serve(stream, &known, &calls));
            }
        });
        let bridge = Self {
            socket,
            program,
            tokens,
        };
        Ok((bridge, received))
    }

    /// A server for one chat's sessions, whose calls carry a new token.
    pub fn server(&self) -> anyhow::Result<(ToolServer, String)> {
        let token = new_token()?;
        let mut tokens = self
            .tokens
            .lock()
            .map_err(|_| anyhow::anyhow!("poisoned"))?;
        tokens.insert(token.clone());
        let socket = self.socket.to_string_lossy().into_owned();
        let server = ToolServer {
            name: SERVER_NAME.to_owned(),
            command: self.program.clone(),
            args: vec!["mcp".to_owned()],
            env: vec![
                (SOCKET_VAR.to_owned(), socket),
                (TOKEN_VAR.to_owned(), token.clone()),
            ],
            commit_directives: Vec::new(),
        };
        Ok((server, token))
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket);
    }
}

fn new_token() -> anyhow::Result<String> {
    let mut bytes = [0; TOKEN_BYTES];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    let mut token = String::with_capacity(TOKEN_BYTES * 2);
    for byte in bytes {
        let _ = write!(token, "{byte:02x}");
    }
    Ok(token)
}

fn serve(stream: UnixStream, tokens: &Mutex<HashSet<String>>, calls: &Sender<Call>) {
    let Ok(reading) = stream.try_clone() else {
        return;
    };
    let mut lines = BufReader::new(reading).lines();
    let Some(Ok(token)) = lines.next() else {
        return;
    };
    let known = tokens.lock().is_ok_and(|tokens| tokens.contains(&token));
    if !known {
        log::warn!("a tool server came with a token Vortex did not give");
        return;
    }
    let (answers, to_send) = async_channel::unbounded::<Value>();
    std::thread::spawn(move || send_all(stream, &to_send));
    for line in lines.map_while(Result::ok) {
        match mcp::read(&line) {
            Incoming::Answer(answer) => drop(answers.send_blocking(answer)),
            Incoming::Call { id, request } => {
                let (reply, replied) = async_channel::bounded(1);
                let token = token.clone();
                if calls
                    .send_blocking(Call {
                        token,
                        request,
                        reply,
                    })
                    .is_err()
                {
                    let gone = Reply::failed("Vortex is closing.");
                    drop(answers.send_blocking(mcp::answer(&id, &gone)));
                    continue;
                }
                let answers = answers.clone();
                // Calls take as long as pages do; others go on meanwhile.
                std::thread::spawn(move || {
                    let reply = replied.recv_blocking();
                    let reply = reply.unwrap_or_else(|_| Reply::failed("The chat is closed."));
                    drop(answers.send_blocking(mcp::answer(&id, &reply)));
                });
            }
            Incoming::Nothing => {}
        }
    }
}

fn send_all(mut stream: UnixStream, answers: &Receiver<Value>) {
    while let Ok(answer) = answers.recv_blocking() {
        if writeln!(stream, "{answer}").is_err() {
            return;
        }
    }
}
