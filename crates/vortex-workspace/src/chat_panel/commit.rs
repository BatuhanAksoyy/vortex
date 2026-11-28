//! The chat's agent asking for a commit: a message that breaks the rules
//! goes back to it, and with `git.commit.review` on the user decides.

use std::path::{Path, PathBuf};

use anyhow::bail;
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
            folder,
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
        let read = cx.background_spawn(async move {
            let folder = commit_folder(&project, folder.as_deref())?;
            Ok((vortex_git::status(&folder)?, folder))
        });
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
        read: anyhow::Result<(Status, PathBuf)>,
        mut asked: Pending,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (status, folder) = match read {
            Ok(read) => read,
            Err(error) => {
                let _ = asked.reply.try_send(Reply::failed(format!("{error:#}")));
                return;
            }
        };
        if folder != asked.project {
            // The push settings are for the project's own branch, not for
            // one the agent made itself.
            asked.push = None;
            asked.project = folder;
        }
        let targets: Vec<_> = (asked.paths.iter())
            .map(|path| target(&status, path))
            .collect();
        asked.files = commit_review::files(&status, &targets);
        if asked.files.is_empty() {
            let nothing = Reply::failed(format!(
                "There is nothing to commit in {}. If the changes are in another checkout, such \
                 as a worktree, give its absolute path as `folder`.",
                asked.project.display()
            ));
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
            cx.spawn(async move |this, cx| {
                answer(commit, reply).await;
                let _ = this.update(cx, Self::recount);
            })
            .detach();
            return;
        }
        let review = cx.new(|cx| CommitReview::new(pending, window, cx));
        let closed = cx.subscribe_in(&review, window, |this, _, event, window, cx| {
            let CommitReviewEvent::Closed { committed } = event;
            this.made_commit |= committed;
            if *committed {
                this.recount(cx);
            }
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

/// Where the agent's commit goes: the project, or `folder`, another
/// checkout of its repository such as a worktree the agent made itself.
fn commit_folder(project: &Path, folder: Option<&str>) -> anyhow::Result<PathBuf> {
    let Some(folder) = folder else {
        return Ok(project.to_path_buf());
    };
    let folder = project.join(folder);
    if !vortex_git::same_repository(project, &folder)? {
        bail!(
            "Nothing was committed: {} is not a checkout of the project's repository.",
            folder.display()
        );
    }
    Ok(folder)
}

/// The agent's `path`, from the repository's top as the status has it. A
/// relative one is from the folder committed in, which may lie below it.
fn target(status: &Status, path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        return path
            .strip_prefix(&status.root)
            .unwrap_or(path)
            .to_path_buf();
    }
    let path = path.strip_prefix("./").unwrap_or(path);
    status.prefix().join(path)
}

/// Pushes as the settings ask after a commit; off the main thread.
fn push_after(project: &Path, push: Option<&vortex_settings::PushSettings>) {
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

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;

    fn git(root: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(["-c", "user.name=Vortex", "-c", "user.email=v@example.com"])
            .args(args)
            .current_dir(root)
            .status()
            .expect("git");
        assert!(status.success(), "git {args:?}");
    }

    #[test]
    fn a_commit_goes_to_a_worktree_of_the_projects_repository_only() {
        let project = tempfile::tempdir().expect("tempdir");
        let root = project.path();
        git(root, &["init", "-q", "-b", "main"]);
        git(root, &["commit", "-q", "--allow-empty", "-m", "start"]);
        let worktree = root.join(".claude/worktrees/fix");
        git(
            root,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "fix",
                &worktree.to_string_lossy(),
            ],
        );
        std::fs::write(worktree.join("new.txt"), "new\n").expect("write");

        assert_eq!(commit_folder(root, None).expect("project"), root);
        let folder = worktree.to_string_lossy();
        let found = commit_folder(root, Some(&folder)).expect("worktree");
        assert_eq!(found, worktree);
        let status = vortex_git::status(&found).expect("status");
        assert_eq!(target(&status, "./new.txt"), Path::new("new.txt"));
        let absolute = status.root.join("new.txt");
        assert_eq!(
            target(&status, &absolute.to_string_lossy()),
            Path::new("new.txt")
        );

        let other = tempfile::tempdir().expect("tempdir");
        git(other.path(), &["init", "-q"]);
        let refused = commit_folder(root, Some(&other.path().to_string_lossy()));
        assert!(refused.is_err(), "another repository");
    }
}
