//! Committing a chat's work for it: the files its turns changed that are
//! not committed yet, with a message the writer model writes from their
//! diff and what the turn was about. With `git.commit.trigger` at
//! `task_done` it is offered once a turn ends with the work done, as the
//! writer judges it; the chat's Commit button asks for it at any time.
//! Either way it waits for the user like the agent's own commit. The work
//! is followed in the project and in the worktrees nested in it, where an
//! agent may work on a branch of its own.

mod checkouts;
mod writing;

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

use gpui_kit::{
    AppContext as _, Context, Subscription, Task, Window,
    component::{WindowExt as _, notification::Notification},
};
use vortex_chat::ChatEvent;
use vortex_git::{MessageRules, Status};
use vortex_settings::{CommitTrigger, PushPolicy, PushSettings, Settings};

use super::ChatPanel;
use crate::commit_review::{self, Pending};
use checkouts::{Snapshot, Work, first_work};
use writing::{Writer, Written};

/// What the files held when the turn started, and what the chat's turns
/// changed since its work was last committed.
#[derive(Default)]
pub(super) struct TurnWatch {
    before: Option<Snapshot>,
    /// By the top folder of the checkout they are in, the paths from it.
    touched: BTreeMap<PathBuf, BTreeSet<PathBuf>>,
    /// The agent committed through Vortex during the turn.
    pub committed: bool,
    work: Option<Task<()>>,
    /// A message is being written, so another commit waits.
    writing: Option<Task<()>>,
    /// Follows the project's git status, to forget what is changed no more.
    repo: Option<Subscription>,
    /// Reads the checkouts again after a commit.
    recount: Option<Task<()>>,
}

/// Why the chat's work is committed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Why {
    /// A turn ended; the writer first judges whether the work is done.
    TurnEnded,
    /// The user pressed Commit & push.
    Asked,
}

impl ChatPanel {
    pub(super) fn follow_turn(
        &mut self,
        event: &ChatEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            ChatEvent::TurnStarted => {
                self.turn.committed = false;
                self.read_before(cx);
            }
            ChatEvent::TurnEnded { completed } => self.collect(*completed, window, cx),
            _ => {}
        }
    }

    /// Forgets a file of the chat's once git no longer sees it changed, as
    /// when the user undid it or committed it elsewhere, so the count is
    /// right between turns too.
    pub(super) fn follow_repo(&mut self, cx: &mut Context<Self>) {
        let repo = self.project.read(cx).repo().clone();
        let follow = cx.subscribe(
            &repo,
            |this, repo, _: &crate::repo_status::StatusRead, cx| {
                if let Some(status) = repo.read(cx).status().cloned() {
                    this.still_changed(std::slice::from_ref(&status), cx);
                }
            },
        );
        self.turn.repo = Some(follow);
    }

    /// The files the chat changed that wait for a commit.
    pub(super) fn touched(&self) -> usize {
        self.turn.touched.values().map(BTreeSet::len).sum()
    }

    /// Keeps of the chat's files in the checkouts `statuses` are of those
    /// still changed there; the other checkouts' stay.
    fn still_changed(&mut self, statuses: &[Status], cx: &mut Context<Self>) {
        let before = self.touched();
        self.turn.touched.retain(|root, paths| {
            let Some(status) = statuses.iter().find(|status| &status.root == root) else {
                return true;
            };
            paths.retain(|path| status.changes.iter().any(|change| &change.path == path));
            !paths.is_empty()
        });
        if self.touched() != before {
            cx.notify();
        }
    }

    /// Forgets what a commit took, whichever checkout it was made in; the
    /// project's own status is followed already, but not a worktree's.
    pub(super) fn recount(&mut self, cx: &mut Context<Self>) {
        self.turn.touched.retain(|root, _| root.exists());
        let roots: Vec<PathBuf> = self.turn.touched.keys().cloned().collect();
        let read = cx.background_spawn(async move {
            let read = roots.iter().map(|root| vortex_git::status(root));
            read.filter_map(Result::ok).collect::<Vec<_>>()
        });
        self.turn.recount = Some(cx.spawn(async move |this, cx| {
            let statuses = read.await;
            let _ = this.update(cx, |this, cx| this.still_changed(&statuses, cx));
        }));
    }

    /// Whether Vortex is writing a commit message for the chat.
    pub(super) fn is_writing(&self) -> bool {
        self.turn.writing.is_some()
    }

    fn read_before(&mut self, cx: &mut Context<Self>) {
        let project = self.project.read(cx).root().to_path_buf();
        let read = cx.background_spawn(async move { checkouts::read(&project) });
        self.turn.work = Some(cx.spawn(async move |this, cx| {
            let before = read.await.ok().map(|statuses| Snapshot::of(&statuses));
            let _ = this.update(cx, |this, cx| this.started_from(before, cx));
        }));
    }

    fn started_from(&mut self, before: Option<Snapshot>, cx: &mut Context<Self>) {
        // What is no longer changed was committed, by any means.
        if let Some(before) = &before {
            self.turn.touched.retain(|root, paths| {
                paths.retain(|path| before.holds(root, path));
                !paths.is_empty()
            });
        }
        self.turn.before = before;
        cx.notify();
    }

    /// Adds what the turn changed to the chat's work; offers to commit it
    /// when the turn finished and the settings ask for that.
    fn collect(&mut self, completed: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(before) = self.turn.before.take() else {
            return;
        };
        let project = self.project.read(cx).root().to_path_buf();
        let read = cx.background_spawn(async move {
            let statuses = checkouts::read(&project)?;
            anyhow::Ok(before.changed_since(&Snapshot::of(&statuses)))
        });
        self.turn.work = Some(cx.spawn_in(window, async move |this, cx| {
            let Ok(changed) = read.await else {
                return;
            };
            let _ = this.update_in(cx, |this, window, cx| {
                this.collected(changed, completed, window, cx);
            });
        }));
    }

    fn collected(
        &mut self,
        changed: Vec<(PathBuf, Vec<PathBuf>)>,
        completed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for (root, paths) in changed {
            self.turn.touched.entry(root).or_default().extend(paths);
        }
        cx.notify();
        let commit = self.settings(cx).git.commit;
        let auto = commit.enabled && commit.trigger == CommitTrigger::TaskDone;
        if completed && auto && !self.turn.committed && self.review.is_none() {
            self.turn.committed = true;
            self.commit_work(Why::TurnEnded, window, cx);
        }
    }

    /// The chat's Commit & push button.
    pub(super) fn commit_asked(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_work(Why::Asked, window, cx);
    }

    fn commit_work(&mut self, why: Why, window: &mut Window, cx: &mut Context<Self>) {
        if self.review.is_some() || self.is_writing() || !self.settings(cx).git.commit.enabled {
            return;
        }
        let settings = self.settings(cx);
        let made = Writer::new(&settings.git.commit, why == Why::TurnEnded).and_then(|writer| {
            let rules = commit_review::message_rules(&settings.git.commit)?;
            Ok((writer, rules))
        });
        let (writer, rules) = match made {
            Ok(made) => made,
            Err(problem) => {
                window.push_notification(Notification::error(problem), cx);
                return;
            }
        };
        let plan = Plan {
            why,
            rules,
            push: push_for(why, &settings),
        };
        let project = self.project.read(cx).root().to_path_buf();
        let touched: Vec<(PathBuf, Vec<PathBuf>)> = (self.turn.touched.iter())
            .map(|(root, paths)| (root.clone(), paths.iter().cloned().collect()))
            .collect();
        let read = cx.background_spawn(async move { first_work(&project, &touched) });
        let (asked, answered) = self.chat.read(cx).last_turn();
        self.turn.writing = Some(cx.spawn_in(window, async move |this, cx| {
            let work = read.await.ok().flatten();
            let written = match &work {
                Some(work) => {
                    let prompt = writing::prompt(&asked, &answered, &work.diff);
                    writer.write(&this, prompt, cx).await
                }
                None => None,
            };
            let _ = this.update_in(cx, |this, window, cx| {
                this.written(work.zip(written), plan, window, cx);
            });
        }));
        cx.notify();
    }

    fn written(
        &mut self,
        done: Option<(Work, Written)>,
        plan: Plan,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.turn.writing = None;
        cx.notify();
        match done {
            Some((work, Written::Message(message))) => {
                let pending = plan.pending(&work, message);
                self.show_review(pending, window, cx);
            }
            None if plan.why == Why::Asked => {
                let nothing = "Nothing this chat changed waits for a commit, or no message came.";
                window.push_notification(Notification::info(nothing), cx);
            }
            Some((_, Written::NotDone)) | None => {}
        }
    }
}

/// What a written message is committed with.
struct Plan {
    why: Why,
    rules: MessageRules,
    push: Option<PushSettings>,
}

impl Plan {
    fn pending(self, work: &Work, message: String) -> Pending {
        // No agent waits for this one.
        let (reply, _) = async_channel::bounded(1);
        let paths = work
            .paths
            .iter()
            .map(|path| path.to_string_lossy().into_owned());
        Pending {
            project: work.status.root.clone(),
            paths: paths.collect(),
            files: commit_review::files(&work.status, &work.paths),
            sent: message.clone(),
            cleaned: message,
            reply,
            rules: self.rules,
            // The push settings are for the project's own branch, not for
            // one the agent made itself.
            push: self.push.filter(|_| !work.nested),
        }
    }
}

/// Where the commit goes once made. Pressing the button says the work is
/// checked, so it goes out unless pushing is off.
fn push_for(why: Why, settings: &Settings) -> Option<PushSettings> {
    let push = &settings.git.push;
    match why {
        Why::Asked => (push.policy != PushPolicy::Never).then(|| push.clone()),
        Why::TurnEnded => crate::pushing::wanted(PushPolicy::AfterCommit, push),
    }
}
