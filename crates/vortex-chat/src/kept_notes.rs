//! What a chat showed that the agent's own record of the conversation
//! leaves out: the words, tokens and time of each thought, how long each
//! turn took, and recaps. The CLI stores few of these (most thoughts without
//! their words, the time of some turns, recaps only sometimes), so Vortex keeps
//! them itself, one file per conversation, and weaves them back in when the
//! conversation is reopened.

mod merge;

use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use serde::{Deserialize, Serialize};
use vortex_agent::{AgentEvent, BlockKey};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Note {
    Thinking {
        block: Block,
        text: String,
        tokens: u64,
        took_ms: Option<u64>,
        /// What came right after it, which finds its place again when the
        /// stored conversation leaves the thought out.
        before: Option<Anchor>,
    },
    TurnTook {
        /// The last message of the turn, by the model's id.
        after: Option<String>,
        took_ms: u64,
        /// Seconds since the Unix epoch.
        ended_at: u64,
    },
    Recap {
        after: Option<String>,
        text: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct Block {
    message: String,
    index: u64,
}

impl From<&BlockKey> for Block {
    fn from(key: &BlockKey) -> Self {
        Self {
            message: key.message_id.clone(),
            index: key.index,
        }
    }
}

impl Block {
    fn key(&self) -> BlockKey {
        BlockKey {
            message_id: self.message.clone(),
            index: self.index,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Anchor {
    Tool { id: String },
    Block { block: Block },
}

/// A conversation's kept notes, and where they go.
#[derive(Default)]
pub struct KeptNotes {
    /// The folder of every conversation's file; `None` keeps nothing.
    dir: Option<PathBuf>,
    /// The conversation they belong to.
    session: Option<String>,
    notes: Vec<Note>,
    /// The model's last message so far, which turn notes follow.
    last_message: Option<String>,
    /// Tokens of a thought reported before its first words.
    unclaimed_tokens: u64,
    changed: bool,
}

impl KeptNotes {
    /// Keeps notes in `dir`, from now on.
    pub fn keep_in(&mut self, dir: PathBuf) {
        self.dir = Some(dir);
    }

    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    /// Reads the notes kept for conversation `session` in `dir`.
    pub fn read(dir: Option<&Path>, session: &str) -> Vec<u8> {
        dir.and_then(|dir| file(dir, session))
            .and_then(|path| std::fs::read(path).ok())
            .unwrap_or_default()
    }

    /// Starts over with what [`Self::read`] read for `session`.
    pub fn restore(&mut self, session: &str, kept: &[u8]) {
        self.notes = serde_json::from_slice(kept).unwrap_or_default();
        self.session = Some(session.to_owned());
        self.last_message = None;
        self.unclaimed_tokens = 0;
        self.changed = false;
    }

    /// Forgets the notes, as the conversation started over.
    pub fn clear(&mut self) {
        let dir = self.dir.take();
        *self = Self {
            dir,
            ..Self::default()
        };
    }

    /// Follows the conversation to the id the agent stores it under; a copy
    /// takes the notes along.
    pub fn follow(&mut self, session: &str) {
        if self.session.as_deref() != Some(session) {
            self.session = Some(session.to_owned());
            self.changed = !self.notes.is_empty();
        }
    }

    pub fn record(&mut self, event: &AgentEvent) {
        match event {
            AgentEvent::ThinkingDelta { block, text } => self.think(block, text),
            AgentEvent::TextDelta { block, .. } => {
                self.close(&Anchor::Block {
                    block: block.into(),
                });
                self.last_message = Some(block.message_id.clone());
            }
            AgentEvent::ToolCall(call) if call.parent.is_none() => {
                self.close(&Anchor::Tool {
                    id: call.id.clone(),
                });
            }
            AgentEvent::ThinkingTokens(tokens) => self.count(*tokens),
            AgentEvent::ThinkingTime {
                message_id,
                duration,
            } => self.time(message_id, *duration),
            AgentEvent::ThoughtTokens { message_id, tokens } => {
                self.counted(message_id, *tokens);
            }
            AgentEvent::TurnTook { duration, ended_at } => {
                let ended_at = ended_at.unwrap_or_else(SystemTime::now);
                self.add(Note::TurnTook {
                    after: self.last_message.clone(),
                    took_ms: millis(*duration),
                    ended_at: seconds(ended_at),
                });
            }
            AgentEvent::Recap(text) => self.add(Note::Recap {
                after: self.last_message.clone(),
                text: text.clone(),
            }),
            _ => {}
        }
    }

    /// The file and what to write to it, when something changed.
    pub fn take_changes(&mut self) -> Option<(PathBuf, Vec<u8>)> {
        if !std::mem::take(&mut self.changed) {
            return None;
        }
        let path = file(self.dir.as_deref()?, self.session.as_deref()?)?;
        Some((path, serde_json::to_vec(&self.notes).ok()?))
    }

    fn think(&mut self, block: &BlockKey, text: &str) {
        self.last_message = Some(block.message_id.clone());
        let block = Block::from(block);
        let open = self.notes.iter_mut().rev().find_map(|note| match note {
            Note::Thinking {
                block: kept,
                text,
                before: None,
                ..
            } if *kept == block => Some(text),
            _ => None,
        });
        if let Some(kept) = open {
            kept.push_str(text);
        } else {
            self.close(&Anchor::Block {
                block: block.clone(),
            });
            let tokens = std::mem::take(&mut self.unclaimed_tokens);
            self.notes.push(Note::Thinking {
                block,
                text: text.to_owned(),
                tokens,
                took_ms: None,
                before: None,
            });
        }
        self.changed = true;
    }

    /// Marks where the thoughts still open end: at `next`.
    fn close(&mut self, next: &Anchor) {
        for note in self.notes.iter_mut().rev() {
            match note {
                Note::Thinking {
                    before: before @ None,
                    block,
                    ..
                } if !matches!(next, Anchor::Block { block: at } if at == block) => {
                    *before = Some(next.clone());
                }
                Note::Thinking { before: None, .. } => {}
                _ => break,
            }
        }
    }

    fn count(&mut self, tokens: u64) {
        match self.notes.last_mut() {
            Some(Note::Thinking {
                tokens: kept,
                before: None,
                ..
            }) => *kept += tokens,
            _ => self.unclaimed_tokens += tokens,
        }
        self.changed = true;
    }

    fn time(&mut self, message_id: &str, duration: Duration) {
        for note in &mut self.notes {
            if let Note::Thinking { block, took_ms, .. } = note
                && block.message == message_id
            {
                *took_ms = Some(millis(duration));
            }
        }
        self.changed = true;
    }

    fn counted(&mut self, message_id: &str, count: u64) {
        for note in &mut self.notes {
            if let Note::Thinking { block, tokens, .. } = note
                && block.message == message_id
            {
                *tokens = count;
            }
        }
        self.changed = true;
    }

    fn add(&mut self, note: Note) {
        self.notes.push(note);
        self.changed = true;
    }
}

/// `None` for an id that is not a plain name, which must not reach
/// outside `dir`.
fn file(dir: &Path, session: &str) -> Option<PathBuf> {
    let plain = !session.is_empty()
        && session
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    plain.then(|| dir.join(format!("{session}.json")))
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn seconds(at: SystemTime) -> u64 {
    at.duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

#[cfg(test)]
mod tests;
