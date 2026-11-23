//! Reopening a stored conversation: its messages come back into the
//! transcript and the next process resumes it.

use gpui_kit::{AppContext as _, Context, EventEmitter, Window};

use super::ChatView;
use crate::transcript::{Status, Transcript};

/// What the window's history list follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatEvent {
    /// Another conversation is shown, or the shown one was saved again.
    SessionChanged,
    /// A message was sent; another conversation cannot be opened until the
    /// turn ends, which `SessionChanged` reports.
    TurnStarted,
}

impl EventEmitter<ChatEvent> for ChatView {}

pub(super) struct Conversation {
    /// The id the agent stores it under, as it last reported it.
    pub id: String,
    /// It is open in another client: the next process continues a copy,
    /// which then reports an id of its own.
    pub fork: bool,
}

impl ChatView {
    /// The conversation shown, by the id the agent stores it under; `None`
    /// until the agent reports one.
    #[must_use]
    pub fn session_id(&self) -> Option<&str> {
        self.conversation.as_ref().map(|stored| stored.id.as_str())
    }

    /// The conversation's name, once something was said.
    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.transcript.title()
    }

    /// Nothing was said or opened here yet, so the chat can take another
    /// conversation without losing one.
    #[must_use]
    pub fn is_blank(&self) -> bool {
        self.conversation.is_none() && self.transcript.len() == 0
    }

    /// Shows stored conversation `id` and continues it, as a copy when it
    /// is `open_elsewhere`: two writers would interleave one file. Not while
    /// a turn runs, which the restart would cut off.
    pub fn open_session(
        &mut self,
        id: &str,
        open_elsewhere: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session_id() == Some(id) || self.status() == Status::Running {
            return;
        }
        self.session = None;
        self.conversation = Some(Conversation {
            id: id.to_owned(),
            fork: open_elsewhere,
        });
        self.transcript = Transcript::default();
        self.toggled.clear();
        self.agent.reset_context();
        self.scroller
            .update(cx, |scroller, cx| scroller.reset(1, cx));
        let load = self.backend.past_messages(&self.cwd, id);
        self.history_load = Some(cx.spawn_in(window, async move |this, cx| {
            let messages = cx.background_spawn(load).await;
            let _ = this.update_in(cx, |this, window, cx| match messages {
                Ok(messages) => this.show_history(messages, window, cx),
                Err(error) => {
                    let change = this
                        .transcript
                        .push_error(format!("Cannot open the conversation: {error:#}"));
                    this.apply_change(change, cx);
                }
            });
        }));
        self.sync_prompt(window, cx);
        self.sync_spinner(cx);
        self.ensure_session(window, cx);
        cx.emit(ChatEvent::SessionChanged);
        cx.notify();
    }

    fn show_history(
        &mut self,
        messages: Vec<kiln_agent::PastMessage>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.transcript.restore(messages);
        if self.conversation.as_ref().is_some_and(|stored| stored.fork) {
            let _ = self.transcript.push_notice(
                "Open in another window. Messages sent here continue a copy of it.".to_owned(),
            );
        }
        let rows = self.transcript.len() + 1;
        self.scroller
            .update(cx, |scroller, cx| scroller.reset(rows, cx));
        self.sync_prompt(window, cx);
        cx.notify();
    }
}
