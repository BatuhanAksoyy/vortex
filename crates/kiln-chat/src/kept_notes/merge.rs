//! Weaves kept notes into a stored conversation: a thought's tokens and
//! time after it, a thought the conversation left out before what followed
//! it, and turn notes at the end of their turn, unless the conversation
//! already holds them.

use std::{
    collections::{HashMap, HashSet},
    time::{Duration, SystemTime},
};

use kiln_agent::{AgentEvent, PastMessage};

use super::{Anchor, Block, KeptNotes, Note};

/// What the stored conversation holds, by position.
struct Stored<'a> {
    messages: &'a [PastMessage],
    thoughts: HashSet<Block>,
    recaps: HashSet<&'a str>,
}

impl<'a> Stored<'a> {
    fn of(messages: &'a [PastMessage]) -> Self {
        let mut thoughts = HashSet::new();
        let mut recaps = HashSet::new();
        for message in messages {
            match message {
                PastMessage::Agent(AgentEvent::ThinkingDelta { block, .. }) => {
                    thoughts.insert(Block::from(block));
                }
                PastMessage::Agent(AgentEvent::Recap(text)) => {
                    recaps.insert(text.as_str());
                }
                _ => {}
            }
        }
        Self {
            messages,
            thoughts,
            recaps,
        }
    }

    fn position(&self, anchor: &Anchor) -> Option<usize> {
        self.messages
            .iter()
            .position(|message| match (message, anchor) {
                (PastMessage::Agent(AgentEvent::ToolCall(call)), Anchor::Tool { id }) => {
                    call.id == *id
                }
                (PastMessage::Agent(event), Anchor::Block { block }) => {
                    block_of(event).is_some_and(|key| key == *block)
                }
                _ => false,
            })
    }

    /// Where the turn holding the model's message `id` ends.
    fn turn_end(&self, id: &str) -> Option<usize> {
        let last = self.messages.iter().rposition(|message| match message {
            PastMessage::Agent(event) => block_of(event).is_some_and(|key| key.message == id),
            PastMessage::User(_) => false,
        })?;
        let next_user = self.messages[last..]
            .iter()
            .position(|message| matches!(message, PastMessage::User(_)));
        Some(next_user.map_or(self.messages.len(), |at| last + at))
    }

    fn took_in_turn(&self, end: usize) -> bool {
        let start = self.messages[..end]
            .iter()
            .rposition(|message| matches!(message, PastMessage::User(_)))
            .unwrap_or(0);
        self.messages[start..end]
            .iter()
            .any(|message| matches!(message, PastMessage::Agent(AgentEvent::TurnTook { .. })))
    }
}

fn block_of(event: &AgentEvent) -> Option<Block> {
    match event {
        AgentEvent::TextDelta { block, .. } | AgentEvent::ThinkingDelta { block, .. } => {
            Some(block.into())
        }
        _ => None,
    }
}

/// Inserted next to stored messages, by their position.
#[derive(Default)]
struct Inserts {
    before: HashMap<usize, Vec<PastMessage>>,
    after: HashMap<usize, Vec<PastMessage>>,
}

impl KeptNotes {
    /// `messages` with the kept notes in their places.
    pub fn merge(&self, messages: Vec<PastMessage>) -> Vec<PastMessage> {
        let stored = Stored::of(&messages);
        let mut inserts = Inserts::default();
        for note in &self.notes {
            place(note, &stored, &mut inserts);
        }
        let end = messages.len();
        let mut merged = Vec::with_capacity(end);
        for (at, message) in messages.into_iter().enumerate() {
            merged.extend(inserts.before.remove(&at).unwrap_or_default());
            merged.push(message);
            merged.extend(inserts.after.remove(&at).unwrap_or_default());
        }
        // A turn that ends the conversation ends after its last message.
        merged.extend(inserts.before.remove(&end).unwrap_or_default());
        merged
    }
}

fn place(note: &Note, stored: &Stored, inserts: &mut Inserts) {
    match note {
        Note::Thinking { .. } => place_thought(note, stored, inserts),
        Note::TurnTook {
            after,
            took_ms,
            ended_at,
        } => {
            let Some(end) = after.as_deref().and_then(|id| stored.turn_end(id)) else {
                return;
            };
            if stored.took_in_turn(end) {
                return;
            }
            let took = AgentEvent::TurnTook {
                duration: Duration::from_millis(*took_ms),
                ended_at: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(*ended_at)),
            };
            inserts
                .before
                .entry(end)
                .or_default()
                .push(PastMessage::Agent(took));
        }
        Note::Recap { after, text } => {
            if stored.recaps.contains(text.as_str()) {
                return;
            }
            let Some(end) = after.as_deref().and_then(|id| stored.turn_end(id)) else {
                return;
            };
            let recap = PastMessage::Agent(AgentEvent::Recap(text.clone()));
            inserts.before.entry(end).or_default().push(recap);
        }
    }
}

/// Its tokens and time after a thought the conversation holds; the whole
/// thought before what followed it, when the conversation left it out.
fn place_thought(note: &Note, stored: &Stored, inserts: &mut Inserts) {
    let Note::Thinking {
        block,
        text,
        tokens,
        took_ms,
        before,
    } = note
    else {
        return;
    };
    let mut told = details(block, *tokens, *took_ms);
    if stored.thoughts.contains(block) {
        let at = stored.position(&Anchor::Block {
            block: block.clone(),
        });
        if let Some(at) = at {
            inserts.after.entry(at).or_default().append(&mut told);
        }
        return;
    }
    let Some(at) = before.as_ref().and_then(|anchor| stored.position(anchor)) else {
        return;
    };
    let thought = PastMessage::Agent(AgentEvent::ThinkingDelta {
        block: block.key(),
        text: text.clone(),
    });
    let slot = inserts.before.entry(at).or_default();
    slot.push(thought);
    slot.append(&mut told);
}

/// A thought's tokens and time, which follow its words.
fn details(block: &Block, tokens: u64, took_ms: Option<u64>) -> Vec<PastMessage> {
    let tokens = (tokens > 0).then_some(AgentEvent::ThinkingTokens(tokens));
    let took = took_ms.map(|took| AgentEvent::ThinkingTime {
        message_id: block.message.clone(),
        duration: Duration::from_millis(took),
    });
    tokens
        .into_iter()
        .chain(took)
        .map(PastMessage::Agent)
        .collect()
}
