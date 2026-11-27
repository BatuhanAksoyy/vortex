//! Carrying a conversation over to another harness: the new one cannot
//! resume what the first stored, so it is told the conversation as Vortex
//! shows it, and continues it as a conversation of its own.

use gpui_kit::{Context, Window};

use super::{ChatEvent, ChatView};
use crate::{
    backends::title,
    transcript::{Entry, Status, Transcript},
};

/// Sent once another account takes over a turn the usage limit cut off.
pub(super) const CARRY_ON: &str = "Continue where you left off.";

/// How much of the conversation the new harness is told, from its end;
/// enough for the recent work without crowding its context.
const MAX_CHARS: usize = 60_000;
/// How much of each tool's output it is told.
const MAX_OUTPUT_CHARS: usize = 600;

impl ChatView {
    /// Continues the conversation with harness `agent` on `account` (`None`
    /// is its usual sign-in): a process of that harness starts a new
    /// conversation, told what was said so far. Not while a turn runs.
    pub(super) fn hand_off(
        &mut self,
        agent: &str,
        account: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.status() == Status::Running {
            return;
        }
        let from = self.backend.name();
        let told = told(&self.transcript, title(from));
        let (mode, effort) = (self.agent.start_mode(), self.agent.start_effort());
        if !self.switch_backend(agent, cx) {
            return;
        }
        if let Some(id) = account {
            self.account = Some(id);
            self.recall_usage(cx);
        }
        if let Some(mode) = mode {
            self.start_in_mode(mode, cx);
        }
        if let Some(effort) = effort {
            self.start_with_effort(effort, cx);
        }
        let blank = self.is_blank();
        self.conversation = None;
        self.handoff = (!blank).then_some(told);
        self.sync_limit_timer(cx);
        if !blank {
            let on = self.account.as_ref().map(|id| self.account_label(id, cx));
            let notice = match on {
                Some(email) => format!("Continued in {} with {email}", title(agent)),
                None => format!("Continued in {}", title(agent)),
            };
            let change = self.transcript.push_notice(notice);
            self.apply_change(change, cx);
        }
        self.ensure_session(window, cx);
        if std::mem::take(&mut self.cut_by_limit) {
            self.submit(CARRY_ON.into(), window, cx);
        }
        cx.emit(ChatEvent::AccountChanged);
        cx.notify();
    }

    /// Moves the chat to account `id` of harness `agent`, or to that
    /// harness's usual sign-in for `None`, and nothing more: a turn the
    /// usage limit cut off waits for the limit notice's button.
    pub(super) fn pick_account(
        &mut self,
        agent: &str,
        id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let cut = std::mem::take(&mut self.cut_by_limit);
        self.continue_with(agent, id, window, cx);
        self.cut_by_limit |= cut;
    }

    /// Moves the chat as [`Self::pick_account`] does and takes up a turn
    /// the usage limit cut off: on its own harness the conversation
    /// resumes, on another one it is handed off.
    pub(super) fn continue_with(
        &mut self,
        agent: &str,
        id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match id {
            Some(id) if agent == self.backend.name() => self.switch_account(&id, window, cx),
            id => self.hand_off(agent, id, window, cx),
        }
    }
}

/// What the new harness is told: the conversation, oldest first, cut at
/// its start to `MAX_CHARS`.
pub(super) fn told(transcript: &Transcript, from: &str) -> String {
    let lines: Vec<String> = (0..transcript.len())
        .filter_map(|index| transcript.entry(index).and_then(|(_, entry)| line(entry)))
        .collect();
    let mut kept = Vec::new();
    let mut size = 0;
    for line in lines.iter().rev() {
        size += line.len() + 2;
        if size > MAX_CHARS {
            kept.push("[Earlier messages left out.]");
            break;
        }
        kept.push(line.as_str());
    }
    kept.reverse();
    format!(
        "The user started this conversation with {from} and continues it with you, in the \
         same folder. It is below, oldest first; tool calls show their input and the start of \
         their output. Carry on from where it ends, without repeating work already done or \
         mentioning the switch unless asked.\n\n<conversation>\n{}\n</conversation>",
        kept.join("\n\n")
    )
}

fn line(entry: &Entry) -> Option<String> {
    match entry {
        Entry::User(message) => Some(format!("User: {}", message.text)),
        Entry::Text { text, .. } => Some(format!("Assistant: {text}")),
        Entry::Tool { call, result } => {
            let output = result.as_ref().map_or_else(String::new, |result| {
                let cut: String = result.output.chars().take(MAX_OUTPUT_CHARS).collect();
                format!("\n{cut}")
            });
            Some(format!("[{}: {}]{output}", call.name, call.input))
        }
        Entry::Thinking { .. } | Entry::Subagent | Entry::Notice { .. } => None,
    }
}
