//! Reopening a stored conversation: its messages come back into the
//! transcript and the next process resumes it.

use std::path::{Path, PathBuf};

use gpui_kit::{AppContext as _, Context, EventEmitter, Window};

use super::ChatView;
use crate::{
    kept_notes::KeptNotes,
    transcript::{Status, Transcript},
};

/// What the window's history list follows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatEvent {
    /// Another conversation is shown, or the shown one was saved again.
    SessionChanged,
    /// A message was sent; another conversation cannot be opened until the
    /// turn ends, which `SessionChanged` reports.
    TurnStarted,
    /// The chat moved to another account.
    AccountChanged,
    /// The permission mode or effort a new process would start with
    /// changed.
    ChoicesChanged,
    /// The conversation's name was clicked: the user wants to switch to
    /// another conversation.
    PickConversation,
    /// The project's name was clicked: the user wants to work in another
    /// folder.
    PickProject,
    /// A file a tool worked on was clicked, to be shown in the editor.
    OpenFile(PathBuf),
}

impl EventEmitter<ChatEvent> for ChatView {}

pub(super) struct Conversation {
    /// The id the agent stores it under, as it last reported it.
    pub id: String,
    /// It is open in another client: the next process continues a copy,
    /// which then reports an id of its own.
    pub fork: bool,
    /// The name the conversations page lists it under, which the CLI keeps
    /// up to date, so the chat shows the same name.
    pub title: Option<String>,
}

impl ChatView {
    /// The conversation shown, by the id the agent stores it under; `None`
    /// until the agent reports one.
    #[must_use]
    pub fn session_id(&self) -> Option<&str> {
        self.conversation.as_ref().map(|stored| stored.id.as_str())
    }

    /// The folder the agent works in.
    #[must_use]
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    /// Whether nothing was said here yet, so the chat can still move to
    /// another folder.
    #[must_use]
    pub fn is_blank(&self) -> bool {
        self.conversation.is_none() && self.transcript.len() == 0
    }

    /// Works in `folder` from now on, with a new conversation.
    pub fn move_to(&mut self, folder: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.cwd = folder;
        self.new_conversation(window, cx);
    }

    /// The agent's process while it runs, to see what runs under it.
    #[must_use]
    pub fn process_id(&self) -> Option<u32> {
        self.session.as_ref()?.pid
    }

    /// Asks for `path`, as a tool call names it, to be shown in the editor.
    pub fn open_file(&mut self, path: &str, cx: &mut Context<Self>) {
        cx.emit(ChatEvent::OpenFile(self.cwd.join(path)));
    }

    /// The conversation's name: the one it is stored under, or before it
    /// is stored, its first message.
    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.conversation
            .as_ref()
            .and_then(|stored| stored.title.as_deref())
            .or_else(|| self.transcript.title())
    }

    /// Names the conversation shown after the stored conversation list.
    pub fn set_stored_title(&mut self, title: Option<&str>, cx: &mut Context<Self>) {
        let Some(stored) = &mut self.conversation else {
            return;
        };
        if stored.title.as_deref() != title {
            stored.title = title.map(str::to_owned);
            cx.notify();
        }
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
            title: None,
        });
        self.transcript = Transcript::default();
        self.toggled.clear();
        self.agent.reset_context();
        self.drop_suggestion(window, cx);
        self.recap = super::recap::RecapWatch::default();
        self.scroller
            .update(cx, |scroller, cx| scroller.reset(1, cx));
        let load = self.backend.past_messages(&self.cwd, id);
        let kept = self.kept_notes(id, cx);
        self.history_load = Some(cx.spawn_in(window, async move |this, cx| {
            let messages = cx.background_spawn(load).await;
            let kept = kept.await;
            let _ = this.update_in(cx, |this, window, cx| match messages {
                Ok(messages) => this.show_history(messages, &kept, window, cx),
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

    /// Keeps what the chat shows that the agent does not store in `dir`,
    /// one file per conversation, to show it again when reopened.
    pub fn keep_notes_in(&mut self, dir: PathBuf) {
        self.kept.keep_in(dir);
    }

    fn kept_notes(&self, id: &str, cx: &Context<Self>) -> gpui_kit::Task<(String, Vec<u8>)> {
        let (dir, id) = (self.kept.dir().map(Path::to_path_buf), id.to_owned());
        cx.background_spawn(async move {
            let kept = KeptNotes::read(dir.as_deref(), &id);
            (id, kept)
        })
    }

    fn show_history(
        &mut self,
        messages: Vec<kiln_agent::PastMessage>,
        (id, kept): &(String, Vec<u8>),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.kept.restore(id, kept);
        self.transcript.restore(self.kept.merge(messages));
        if self.conversation.as_ref().is_some_and(|stored| stored.fork) {
            let _ = self.transcript.push_notice(
                "Open in another window. Messages sent here continue a copy of it.".to_owned(),
            );
        }
        let rows = self.transcript.len() + 1;
        self.scroller
            .update(cx, |scroller, cx| scroller.reset(rows, cx));
        self.refresh_find(false, cx);
        self.sync_prompt(window, cx);
        cx.notify();
    }
}
