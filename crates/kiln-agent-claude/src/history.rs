//! The conversations the CLI stores under `<config>/projects/<cwd>/`, one
//! `<session id>.jsonl` per conversation, listed the way the VS Code
//! extension lists them.

mod cache;
mod messages;
mod running;
mod subagents;

use std::{
    fs::File,
    io::{Read as _, Seek as _, SeekFrom},
    path::{Path, PathBuf},
    time::SystemTime,
};

use anyhow::Context as _;
use kiln_agent::{PastMessage, PastSession};
use serde_json::Value;

pub use cache::last_prompt_tokens;
pub use messages::messages;
pub use running::open_elsewhere;

/// How much of each file's start and end the list reads, as the extension
/// does: titles and the last prompt are written as the conversation goes,
/// so the end holds the latest.
const PEEK_BYTES: u64 = 64 * 1024;
/// The CLI cuts longer directory names and appends a hash of the path.
const MAX_DIR_NAME: usize = 200;
const MAX_TITLE_CHARS: usize = 200;

/// Where the CLI keeps its state.
#[must_use]
pub fn config_dir() -> Option<PathBuf> {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::home_dir().map(|home| home.join(".claude")))
}

/// The directories that may hold `cwd`'s conversations: the CLI names them
/// after the path it was started in, which may be `cwd` as given or with
/// its links resolved (`/tmp` is `/private/tmp` on macOS).
#[must_use]
pub fn project_dirs(config: &Path, cwd: &Path) -> Vec<PathBuf> {
    let projects = config.join("projects");
    let mut dirs: Vec<PathBuf> = [Some(cwd.to_path_buf()), std::fs::canonicalize(cwd).ok()]
        .into_iter()
        .flatten()
        .filter_map(|path| project_dir(&projects, &path))
        .collect();
    dirs.dedup();
    dirs
}

/// Every character but ASCII letters and digits becomes `-`, one per UTF-16
/// unit, as the CLI's JavaScript replace does.
fn project_dir(projects: &Path, cwd: &Path) -> Option<PathBuf> {
    let name: String = cwd
        .to_string_lossy()
        .chars()
        .flat_map(|c| {
            let kept = if c.is_ascii_alphanumeric() { c } else { '-' };
            std::iter::repeat_n(kept, if kept == c { 1 } else { c.len_utf16() })
        })
        .collect();
    if name.len() <= MAX_DIR_NAME {
        return Some(projects.join(name));
    }
    // The hash is the CLI's own string hash; the cut name finds the directory.
    let prefix = format!("{}-", &name[..MAX_DIR_NAME]);
    std::fs::read_dir(projects)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with(&prefix))
        })
}

/// The conversations in `dirs`, most recently active first.
pub fn list(dirs: &[PathBuf]) -> anyhow::Result<Vec<PastSession>> {
    let mut sessions: Vec<PastSession> = Vec::new();
    for dir in dirs {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(error).with_context(|| format!("cannot read {}", dir.display()));
            }
        };
        let found = entries.flatten().filter_map(|entry| listed(&entry.path()));
        for session in found {
            if !sessions.iter().any(|known| known.id == session.id) {
                sessions.push(session);
            }
        }
    }
    sessions.sort_by_key(|session| std::cmp::Reverse(session.updated_at));
    Ok(sessions)
}

/// The messages of conversation `id`, from the first of `dirs` that has it,
/// with what its subagents did on their cards.
pub fn read(dirs: &[PathBuf], id: &str) -> anyhow::Result<Vec<PastMessage>> {
    let (dir, text) = stored(dirs, id)?;
    let agents = subagents::load(&dir.join(id).join("subagents"));
    Ok(subagents::weave(messages(&text), agents))
}

/// The file of conversation `id`, from the first of `dirs` that has it, and
/// the directory it is in.
fn stored<'a>(dirs: &'a [PathBuf], id: &str) -> anyhow::Result<(&'a Path, String)> {
    let dir = dirs
        .iter()
        .find(|dir| dir.join(format!("{id}.jsonl")).is_file())
        .with_context(|| format!("no stored conversation {id}"))?;
    let path = dir.join(format!("{id}.jsonl"));
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("cannot read {}", path.display()))?;
    Ok((dir, text))
}

fn listed(path: &Path) -> Option<PastSession> {
    let id = path.file_name()?.to_str()?.strip_suffix(".jsonl")?;
    let (head, tail, modified) = peek(path).ok()?;
    let title = title(&head, &tail)?;
    Some(PastSession {
        id: id.to_owned(),
        title,
        updated_at: last_message_at(&tail).unwrap_or(modified),
        open_elsewhere: false,
    })
}

/// The file's first and last `PEEK_BYTES`; lines cut at either edge do not
/// parse and are skipped.
fn peek(path: &Path) -> std::io::Result<(String, String, SystemTime)> {
    let mut file = File::open(path)?;
    let metadata = file.metadata()?;
    let mut head = Vec::new();
    (&mut file).take(PEEK_BYTES).read_to_end(&mut head)?;
    let head = String::from_utf8_lossy(&head).into_owned();
    let tail = if metadata.len() > PEEK_BYTES {
        file.seek(SeekFrom::End(
            -i64::try_from(PEEK_BYTES).unwrap_or(i64::MAX),
        ))?;
        let mut tail = Vec::new();
        file.read_to_end(&mut tail)?;
        String::from_utf8_lossy(&tail).into_owned()
    } else {
        head.clone()
    };
    Ok((head, tail, metadata.modified()?))
}

/// The extension's order: a title the user gave, the one the CLI wrote,
/// the last prompt, then the first. `None` for a subagent's conversation or
/// one without a message.
fn title(head: &str, tail: &str) -> Option<String> {
    let first = lines(head).next();
    if first.is_some_and(|line| line.get("isSidechain") == Some(&Value::Bool(true))) {
        return None;
    }
    let latest = |kind: &str, field: &str| {
        last_string(tail, kind, field).or_else(|| last_string(head, kind, field))
    };
    let title = latest("custom-title", "customTitle")
        .or_else(|| latest("ai-title", "aiTitle"))
        .or_else(|| last_string(tail, "last-prompt", "lastPrompt"))
        .or_else(|| first_prompt(head))?;
    Some(shorten(&title)).filter(|title| !title.is_empty())
}

/// When the last message was written. Not the file's modified time: a
/// resumed process appends its bookkeeping as soon as it starts, before
/// anything is said.
fn last_message_at(tail: &str) -> Option<SystemTime> {
    lines(tail).rev().find_map(|line| {
        if !matches!(line.get("type")?.as_str()?, "user" | "assistant") {
            return None;
        }
        let stamp: jiff::Timestamp = line.get("timestamp")?.as_str()?.parse().ok()?;
        Some(SystemTime::from(stamp))
    })
}

fn lines(text: &str) -> impl DoubleEndedIterator<Item = Value> + '_ {
    text.lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
}

/// The last non-empty `field` of a line of type `kind`.
fn last_string(text: &str, kind: &str, field: &str) -> Option<String> {
    lines(text).rev().find_map(|line| {
        (line.get("type")?.as_str()? == kind)
            .then(|| line.get(field)?.as_str().map(str::to_owned))
            .flatten()
            .filter(|value| !value.trim().is_empty())
    })
}

fn first_prompt(head: &str) -> Option<String> {
    lines(head)
        .filter_map(|line| messages::user_prompt(&line))
        .find(|prompt| !prompt.trim().is_empty())
}

/// One line, at most `MAX_TITLE_CHARS`.
fn shorten(text: &str) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= MAX_TITLE_CHARS {
        return line;
    }
    let cut: String = line.chars().take(MAX_TITLE_CHARS).collect();
    format!("{}…", cut.trim_end())
}

#[cfg(test)]
mod tests;
