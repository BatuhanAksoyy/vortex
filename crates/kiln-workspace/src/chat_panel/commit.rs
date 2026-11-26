//! The chat's agent asking for a commit: a message that breaks the rules
//! goes back to it, and with `git.commit.review` on the user decides.

use async_channel::Sender;
use gpui_kit::prelude::*;
use gpui_kit::{Context, Focusable as _, Window};
use kiln_bridge::Reply;
use kiln_git::Status;

use super::ChatPanel;
use crate::commit_review::{self, CommitCall, CommitReview, CommitReviewEvent, Pending};

impl ChatPanel {
    pub fn ask_commit(&mut self, call: CommitCall, window: &mut Window, cx: &mut Context<Self>) {
        let CommitCall {
            message,
            paths,
            reply,
        } = call;
        if self.review.is_some() {
            let _ = reply.try_send(Reply::failed(
                "Another commit from this chat waits for the user.",
            ));
            return;
        }
        let cleaned = match commit_review::checked(&message, cx) {
            Ok(cleaned) => cleaned,
            Err(problems) => {
                let _ = reply.try_send(Reply::failed(format!(
                    "Nothing was committed: {problems} Fix the message and call commit again."
                )));
                return;
            }
        };
        let project = self.project.read(cx).root().to_path_buf();
        if !crate::config::settings(cx).git.commit.review {
            let commit = cx.background_spawn(async move {
                let made = kiln_git::commit_paths(&project, &cleaned, &paths);
                made.map(|id| commit_review::committed_reply(&id, &cleaned, &message))
            });
            cx.background_spawn(answer(commit, reply)).detach();
            return;
        }
        let asked = Pending {
            project: project.clone(),
            paths,
            files: Vec::new(),
            sent: message,
            cleaned,
            reply,
        };
        let read = cx.background_spawn(async move { kiln_git::status(&project) });
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
        asked.files = commit_review::files(&status, &asked.paths);
        if asked.files.is_empty() || self.review.is_some() {
            let _ = asked
                .reply
                .try_send(Reply::failed(if asked.files.is_empty() {
                    "There is nothing to commit there."
                } else {
                    "Another commit from this chat waits for the user."
                }));
            return;
        }
        let review = cx.new(|cx| CommitReview::new(asked, window, cx));
        let closed = cx.subscribe_in(&review, window, |this, _, event, window, cx| {
            let CommitReviewEvent::Closed = event;
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

/// Hands the agent what came of a commit made without review.
async fn answer(commit: gpui_kit::Task<anyhow::Result<Reply>>, reply: Sender<Reply>) {
    let answer = match commit.await {
        Ok(answer) => answer,
        Err(error) => Reply::failed(format!("Nothing was committed: {error:#}")),
    };
    let _ = reply.try_send(answer);
}
