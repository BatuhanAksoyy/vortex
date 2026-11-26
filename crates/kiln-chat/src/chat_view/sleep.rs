//! A chat left idle long enough lets its agent's process go, so idle chats
//! hold no memory or connections; the next message starts it again on the
//! same conversation. A chat with a turn, a subagent, a background task or
//! a question running stays awake: those would end with the process.

use std::time::Duration;

use gpui_kit::{Context, Task};
use kiln_agent::AgentEvent;

use super::ChatView;
use crate::transcript::Status;

#[derive(Default)]
pub(super) struct Sleep {
    after: Option<Duration>,
    /// Tasks the agent runs in the background, as it last said.
    background: usize,
    timer: Option<Task<()>>,
}

/// What an event says of sleeping, kept before the event is used up.
#[derive(Clone, Copy)]
pub(super) struct Signal {
    background: Option<usize>,
    /// The idle time counts from the turn's end, and from the last
    /// background task's.
    restart: bool,
}

impl Signal {
    pub(super) fn of(event: &AgentEvent) -> Self {
        let background = match event {
            AgentEvent::BackgroundTasks(running) => Some(*running),
            _ => None,
        };
        let restart = matches!(event, AgentEvent::TurnEnded(_)) || background == Some(0);
        Self {
            background,
            restart,
        }
    }
}

impl ChatView {
    /// How long the chat may stay idle before its process goes; `None`
    /// keeps it running.
    pub fn set_sleep_after(&mut self, after: Option<Duration>, cx: &mut Context<Self>) {
        if self.sleep.after != after {
            self.sleep.after = after;
            self.sync_sleep(true, cx);
        }
    }

    /// Whether the chat let its process go and waits for a message.
    #[must_use]
    pub fn is_asleep(&self) -> bool {
        self.session.is_none() && self.conversation.is_some()
    }

    pub(super) fn follow_sleep(&mut self, signal: Signal, cx: &mut Context<Self>) {
        if let Some(running) = signal.background {
            self.sleep.background = running;
        }
        self.sync_sleep(signal.restart, cx);
    }

    fn sync_sleep(&mut self, restart: bool, cx: &mut Context<Self>) {
        let Some(after) = self.sleep.after else {
            self.sleep.timer = None;
            return;
        };
        if self.status() == Status::Running || self.session.is_none() {
            self.sleep.timer = None;
            return;
        }
        if self.sleep.timer.is_some() && !restart {
            return;
        }
        self.sleep.timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(after).await;
            let _ = this.update(cx, Self::fall_asleep);
        }));
    }

    fn fall_asleep(&mut self, cx: &mut Context<Self>) {
        self.sleep.timer = None;
        let busy = self.status() == Status::Running
            || self.sleep.background > 0
            || self.transcript.subagents().running() > 0
            || self.transcript.pending_permission().is_some();
        if busy || self.session.is_none() {
            return;
        }
        // Dropping the session ends the process.
        self.session = None;
        cx.notify();
    }
}
