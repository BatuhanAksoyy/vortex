//! The chat's agent asking for a commit: a message that breaks the rules
//! goes back to it, and with `git.commit.review` on the user decides.

use async_channel::Sender;
use gpui_kit::prelude::*;
use gpui_kit::{Context, Focusable as _, Window};
use vortex_bridge::Reply;
use vortex_git::Status;
use vortex_settings::PushPolicy;

use super::ChatPanel;
use crate::commit_review::{self, CommitCall, CommitReview, CommitReviewEvent, Pending};

const WAITING: &str = "Another commit from this chat waits for the user.";

impl ChatPanel {
    pub fn ask_commit(&mut self, call: CommitCall, window: &mut Window, cx: &mut Context<Self>) {
        let CommitCall {
            message,
            paths,
            reply,
        } = call;
        self.turn.committed = true;
        if self.review.is_some() {
            let _ = reply.try_send(Reply::failed(WAITING));
            return;
        }
        let settings = self.settings(cx);
        let checked = commit_review::message_rules(&settings.git.commit)
            .and_then(|rules| Ok((commit_review::checked(&message, &rules)?, rules)));
        let (cleaned, rules) = match checked {
            Ok(checked) => checked,
            Err(problems) => {
                let _ = reply.try_send(Reply::failed(format!(
                    "Nothing was committed: {problems} Fix the message and call commit again."
                )));
                return;
            }
        };
        let project = self.project.read(cx).root().to_path_buf();
        let asked = Pending {
            project: project.clone(),
            paths,
            files: Vec::new(),
            sent: message,
            cleaned,
            reply,
            rules,
            push: crate::pushing::wanted(PushPolicy::AfterCommit, &settings.git.push),
        };
        let read = cx.background_spawn(async move { vortex_git::status(&project) });
        cx.spawn_in(window, async move |this, cx| {
            let status = read.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.open_review(status, asked, window, cx);
            });
        })
        .detach();
    }

    fn open_review(
        &mut self,
        status: anyhow::Result<Status>,
        mut asked: Pending,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let status = match status {
            Ok(status) => status,
            Err(error) => {
                let _ = asked.reply.try_send(Reply::failed(format!("{error:#}")));
                return;
            }
        };
        // The agent's paths are from the project folder, the status's from
        // the repository's top.
        let targets: Vec<_> = (asked.paths.iter())
            .map(|path| status.prefix().join(path.trim_start_matches("./")))
            .collect();
        asked.files = commit_review::files(&status, &targets);
        if asked.files.is_empty() {
            let nothing = Reply::failed("There is nothing to commit there.");
            let _ = asked.reply.try_send(nothing);
            return;
        }
        self.show_review(asked, window, cx);
    }

    /// Lets the user decide on `pending`, or commits it at once when the
    /// settings leave it to Vortex.
    pub(super) fn show_review(
        &mut self,
        pending: Pending,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.review.is_some() {
            let _ = pending.reply.try_send(Reply::failed(WAITING));
            return;
        }
        if !self.settings(cx).git.commit.review {
            let Pending {
                project,
                paths,
                sent,
                cleaned,
                reply,
                push,
                ..
            } = pending;
            self.made_commit = true;
            let commit = cx.background_spawn(async move {
                let id = vortex_git::commit_paths(&project, &cleaned, &paths)?;
                push_after(&project, push.as_ref());
                Ok(commit_review::committed_reply(&id, &cleaned, &sent))
            });
            cx.background_spawn(answer(commit, reply)).detach();
            return;
        }
        let review = cx.new(|cx| CommitReview::new(pending, window, cx));
        let closed = cx.subscribe_in(&review, window, |this, _, event, window, cx| {
            let CommitReviewEvent::Closed { committed } = event;
            this.made_commit |= committed;
            this.review = None;
            this.chat.focus_handle(cx).focus(window, cx);
            cx.notify();
        });
        // Only where the user is: the agent must not take the keyboard from
        // another chat.
        if self.chat.focus_handle(cx).contains_focused(window, cx) {
            let message = review.read(cx).message();
            message.update(cx, |message, cx| message.focus(window, cx));
        }
        self.review = Some((review, closed));
        cx.notify();
    }
}

/// Pushes as the settings ask after a commit; off the main thread.
fn push_after(project: &std::path::Path, push: Option<&vortex_settings::PushSettings>) {
    let Some(push) = push else {
        return;
    };
    if let Err(error) = crate::pushing::push_unattended(project, push) {
        log::warn!("cannot push {}: {error:#}", project.display());
    }
}

/// Hands the agent what came of a commit made without review.
async fn answer(commit: gpui_kit::Task<anyhow::Result<Reply>>, reply: Sender<Reply>) {
    let answer = match commit.await {
        Ok(answer) => answer,
        Err(error) => {
            log::warn!("a commit failed: {error:#}");
            Reply::failed(format!("Nothing was committed: {error:#}"))
        }
    };
    let _ = reply.try_send(answer);
}
