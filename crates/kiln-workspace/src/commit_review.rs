//! A commit an agent asks for with Kiln's `commit` tool, over its chat until
//! the user commits or rejects it. The files it takes are listed, and the
//! message can be edited first; it is held to the settings' rules again on
//! the way out.

use std::path::{Path, PathBuf};

use async_channel::Sender;
use gpui_kit::prelude::*;
use gpui_kit::{
    App, Context, Entity, EventEmitter, FocusHandle, FontWeight, KeyDownEvent, SharedString,
    Subscription, Task, TestSupportExt, Window,
    component::{
        ActiveTheme, Disableable as _,
        button::{Button, ButtonVariants as _},
        h_flex,
        input::{InputEvent, Textarea, TextareaState},
        v_flex,
    },
    div, px,
};
use kiln_bridge::Reply;
use kiln_chat::scale::{ScaledButton as _, ScaledText as _};
use kiln_git::{MessageRules, Status};

const WIDTH: f32 = 420.;
const FILES_HEIGHT: f32 = 160.;
const MESSAGE_ROWS: (usize, usize) = (3, 10);

/// The rules from the settings.
///
/// # Errors
/// When one of the settings' patterns is not a valid regular expression.
pub(crate) fn message_rules(cx: &App) -> Result<MessageRules, String> {
    let commit = &crate::config::settings(cx).git.commit;
    MessageRules::new(&commit.strip, commit.subject_max, commit.conventional)
        .map_err(|error| format!("git.commit.strip: {error}"))
}

/// `message` cleaned, or what keeps it from being committed.
pub(crate) fn checked(message: &str, cx: &App) -> Result<String, String> {
    let rules = message_rules(cx)?;
    let message = rules.clean(message);
    let problems = rules.problems(&message);
    if problems.is_empty() {
        Ok(message)
    } else {
        Err(problems.join(" "))
    }
}

/// A file the commit takes, with its lines added and removed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct File {
    pub path: String,
    pub letter: char,
    pub added: usize,
    pub removed: usize,
}

/// The changes in `status` under `targets`, paths from the repository's
/// top that may be folders; every change when there are none.
pub(crate) fn files(status: &Status, targets: &[PathBuf]) -> Vec<File> {
    let wanted = |path: &Path| targets.is_empty() || targets.iter().any(|t| path.starts_with(t));
    let mut files: Vec<File> = Vec::new();
    for change in &status.changes {
        let original = change.original.as_deref();
        if !wanted(&change.path) && !original.is_some_and(wanted) {
            continue;
        }
        let path = change.path.to_string_lossy().into_owned();
        let counts = status.counts(change);
        let (added, removed) = counts.map_or((0, 0), |c| (c.added, c.removed));
        match files.iter_mut().find(|file| file.path == path) {
            // Staged and changed again since: one file, all its lines.
            Some(file) => {
                file.added += added;
                file.removed += removed;
            }
            None => files.push(File {
                path,
                letter: change.kind.letter(),
                added,
                removed,
            }),
        }
    }
    files
}

pub enum CommitReviewEvent {
    Closed,
}

pub struct CommitReview {
    project: PathBuf,
    paths: Vec<String>,
    files: Vec<File>,
    /// What the agent sent, to tell it when the commit says otherwise.
    asked: String,
    message: Entity<TextareaState>,
    reply: Sender<Reply>,
    error: Option<SharedString>,
    committing: Option<Task<()>>,
    focus: FocusHandle,
    _typed: Subscription,
}

impl EventEmitter<CommitReviewEvent> for CommitReview {}

/// The agent's call, as it came.
pub struct CommitCall {
    pub message: String,
    pub paths: Vec<String>,
    pub reply: Sender<Reply>,
}

/// A call whose message passed the rules, with the files it takes.
pub(crate) struct Pending {
    pub project: PathBuf,
    pub paths: Vec<String>,
    pub files: Vec<File>,
    pub sent: String,
    pub cleaned: String,
    pub reply: Sender<Reply>,
}

impl CommitReview {
    pub(crate) fn new(asked: Pending, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let cleaned = asked.cleaned;
        let message = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(MESSAGE_ROWS.0, MESSAGE_ROWS.1)
                .default_value(cleaned)
        });
        let typed = cx.subscribe_in(&message, window, |this, _, event, window, cx| {
            if let InputEvent::PressEnter {
                secondary: true, ..
            } = event
            {
                this.commit(window, cx);
            }
        });
        Self {
            project: asked.project,
            paths: asked.paths,
            files: asked.files,
            asked: asked.sent,
            message,
            reply: asked.reply,
            error: None,
            committing: None,
            focus: cx.focus_handle(),
            _typed: typed,
        }
    }

    pub(crate) fn message(&self) -> Entity<TextareaState> {
        self.message.clone()
    }

    fn commit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.committing.is_some() {
            return;
        }
        let message = match checked(&self.message.read(cx).value(), cx) {
            Ok(message) => message,
            Err(problems) => {
                self.error = Some(problems.into());
                cx.notify();
                return;
            }
        };
        let (project, paths) = (self.project.clone(), self.paths.clone());
        let sent = message.clone();
        let running =
            cx.background_spawn(async move { kiln_git::commit_paths(&project, &sent, &paths) });
        self.committing = Some(cx.spawn_in(window, async move |this, cx| {
            let result = running.await.map_err(|error| format!("{error:#}"));
            let _ = this.update(cx, |this, cx| this.committed(result, &message, cx));
        }));
        cx.notify();
    }

    fn committed(&mut self, result: Result<String, String>, message: &str, cx: &mut Context<Self>) {
        self.committing = None;
        match result {
            Ok(id) => {
                let _ = self
                    .reply
                    .try_send(committed_reply(&id, message, &self.asked));
                cx.emit(CommitReviewEvent::Closed);
            }
            // A hook refused, say: the user may fix it and try again.
            Err(error) => self.error = Some(error.into()),
        }
        cx.notify();
    }

    fn reject(&mut self, cx: &mut Context<Self>) {
        let _ = self.reply.try_send(Reply::failed(
            "The user rejected the commit; nothing was committed. Ask them what they want changed.",
        ));
        cx.emit(CommitReviewEvent::Closed);
    }

    fn on_key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key == "escape" {
            cx.stop_propagation();
            self.reject(cx);
        }
    }
}

/// What the agent hears of a commit made: its id, and the message when it
/// is not the one sent.
pub(crate) fn committed_reply(id: &str, message: &str, asked: &str) -> Reply {
    let short = id.get(..7).unwrap_or(id);
    let subject = message.lines().next().unwrap_or_default();
    if message == asked.trim() {
        Reply::text(format!("Committed {short}: {subject}"))
    } else {
        Reply::text(format!("Committed {short} with this message:\n\n{message}"))
    }
}

impl Render for CommitReview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let count = self.files.len();
        let title = if count == 1 {
            "Commit 1 file?".to_owned()
        } else {
            format!("Commit {count} files?")
        };
        v_flex()
            .id("commit-review")
            .test_support()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .w(px(WIDTH))
            .max_w_full()
            .gap_3()
            .p_4()
            .rounded(theme.radius_lg)
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .text_color(theme.popover_foreground)
            .shadow_lg()
            .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
            .child(self.render_files(cx))
            .child(Textarea::new(&self.message).aria_label("Commit message"))
            .children(self.error.clone().map(|error| {
                div()
                    .id("commit-review-error")
                    .test_support()
                    .aria_label(error.clone())
                    .text_caption()
                    .text_color(theme.danger)
                    .child(error)
            }))
            .child(self.render_buttons(cx))
    }
}

impl CommitReview {
    fn render_files(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let rows = self.files.iter().map(|file| {
            h_flex()
                .gap_2()
                .text_caption()
                .child(
                    div()
                        .flex_none()
                        .w_4()
                        .text_color(theme.muted_foreground)
                        .child(file.letter.to_string()),
                )
                .child(div().flex_1().min_w_0().truncate().child(file.path.clone()))
                .child(
                    div()
                        .flex_none()
                        .text_color(theme.success)
                        .child(format!("+{}", file.added)),
                )
                .child(
                    div()
                        .flex_none()
                        .text_color(theme.danger)
                        .child(format!("−{}", file.removed)),
                )
        });
        v_flex()
            .id("commit-review-files")
            .test_support()
            .max_h(px(FILES_HEIGHT))
            .overflow_y_scroll()
            .gap_0p5()
            .children(rows)
    }

    fn render_buttons(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        h_flex()
            .justify_end()
            .gap_2()
            .child(
                Button::new("commit-review-reject")
                    .control()
                    .label("Reject")
                    .disabled(self.committing.is_some())
                    .on_click(cx.listener(|this, _, _, cx| this.reject(cx))),
            )
            .child(
                Button::new("commit-review-commit")
                    .primary()
                    .control()
                    .label("Commit")
                    .loading(self.committing.is_some())
                    .on_click(cx.listener(|this, _, window, cx| this.commit(window, cx))),
            )
    }
}

#[cfg(test)]
mod tests;
