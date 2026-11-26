//! With `git.commit.trigger` at `turn_end`, a turn that changed files and
//! did not commit them through Kiln's tool gets a commit of its own: the
//! files the turn changed, with a message a small model writes from their
//! diff and what the turn was about. It waits for the user like the agent's.

use std::{fmt::Write as _, path::PathBuf};

use gpui_kit::{App, AppContext as _, AsyncWindowContext, Context, Task, WeakEntity, Window};
use kiln_chat::ChatEvent;
use kiln_git::{Fingerprint, MessageRules, Status};
use kiln_settings::CommitTrigger;

use super::ChatPanel;
use crate::commit_review::{self, Pending};

/// The most of the diff the model reads; enough to tell what a turn did.
const MAX_DIFF: usize = 30_000;

/// What the files held when the turn started, and whether the agent
/// committed through Kiln since.
#[derive(Default)]
pub(super) struct TurnWatch {
    before: Option<Fingerprint>,
    pub committed: bool,
    work: Option<Task<()>>,
}

/// What a finished turn changed.
struct TurnChanges {
    status: Status,
    paths: Vec<PathBuf>,
    diff: String,
}

impl ChatPanel {
    pub(super) fn follow_turn(
        &mut self,
        event: &ChatEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let on = crate::config::settings(cx).git.commit.trigger == CommitTrigger::TurnEnd;
        match event {
            ChatEvent::TurnStarted => {
                self.turn = TurnWatch::default();
                if on {
                    self.read_before(cx);
                }
            }
            ChatEvent::TurnCompleted if on => self.turn_completed(window, cx),
            _ => {}
        }
    }

    fn read_before(&mut self, cx: &mut Context<Self>) {
        let project = self.project.read(cx).root().to_path_buf();
        let read = cx.background_spawn(async move { kiln_git::status(&project) });
        self.turn.work = Some(cx.spawn(async move |this, cx| {
            let before = read.await.ok().map(|status| Fingerprint::of(&status));
            let _ = this.update(cx, |this, _| this.turn.before = before);
        }));
    }

    fn turn_completed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(before) = self.turn.before.take() else {
            return;
        };
        if self.turn.committed || self.review.is_some() {
            return;
        }
        let project = self.project.read(cx).root().to_path_buf();
        let changes = cx.background_spawn(async move { turn_changes(&project, &before) });
        self.turn.work = Some(cx.spawn_in(window, async move |this, cx| {
            let Ok(Some(changes)) = changes.await else {
                return;
            };
            let _ = this.update_in(cx, |this, window, cx| {
                this.write_message(changes, window, cx);
            });
        }));
    }

    /// Asks the model for a message, then hands the commit on.
    fn write_message(&mut self, changes: TurnChanges, window: &mut Window, cx: &mut Context<Self>) {
        let Ok(rules) = commit_review::message_rules(cx) else {
            return;
        };
        let (asked, answered) = self.chat.read(cx).last_turn();
        let writer = Writer {
            model: crate::config::settings(cx).git.commit.model.clone(),
            system: instructions(&rules),
            prompt: prompt(&asked, &answered, &changes.diff),
            rules,
        };
        self.turn.work = Some(cx.spawn_in(window, async move |this, cx| {
            let Some(message) = written(&this, &writer, cx).await else {
                return;
            };
            let _ = this.update_in(cx, |this, window, cx| {
                this.commit_turn(&changes, message, window, cx);
            });
        }));
    }

    fn commit_turn(
        &mut self,
        changes: &TurnChanges,
        message: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // No agent waits for this one.
        let (reply, _) = async_channel::bounded(1);
        let paths = changes
            .paths
            .iter()
            .map(|path| path.to_string_lossy().into_owned());
        let pending = Pending {
            project: changes.status.root.clone(),
            paths: paths.collect(),
            files: commit_review::files(&changes.status, &changes.paths),
            sent: message.clone(),
            cleaned: message,
            reply,
        };
        self.show_review(pending, window, cx);
    }
}

/// The changed files `before` did not hold as they are now, and their diff;
/// `None` when the turn changed none.
fn turn_changes(
    project: &std::path::Path,
    before: &Fingerprint,
) -> anyhow::Result<Option<TurnChanges>> {
    let status = kiln_git::status(project)?;
    let paths = before.changed_since(&Fingerprint::of(&status));
    if paths.is_empty() {
        return Ok(None);
    }
    let mut diff = String::new();
    let changes = status
        .changes
        .iter()
        .filter(|change| paths.contains(&change.path));
    for change in changes {
        if diff.len() >= MAX_DIFF {
            break;
        }
        diff.push_str(&kiln_git::diff(&status.root, change)?);
    }
    if let Some((end, _)) = diff.char_indices().nth(MAX_DIFF) {
        diff.truncate(end);
    }
    Ok(Some(TurnChanges {
        status,
        paths,
        diff,
    }))
}

/// How the message is asked for.
struct Writer {
    model: String,
    system: String,
    prompt: String,
    rules: MessageRules,
}

impl Writer {
    fn ask(&self, panel: &ChatPanel, prompt: String, cx: &App) -> kiln_agent::Pending<String> {
        let chat = panel.chat.read(cx);
        chat.ask_once(&self.model, &self.system, prompt, cx)
    }
}

/// The model's message, cleaned; asked once more if it breaks the rules.
async fn written(
    this: &WeakEntity<ChatPanel>,
    writer: &Writer,
    cx: &mut AsyncWindowContext,
) -> Option<String> {
    let first = this.update(cx, |this, cx| writer.ask(this, writer.prompt.clone(), cx));
    let message = writer.rules.clean(&first.ok()?.await.ok()?);
    let problems = writer.rules.problems(&message);
    if problems.is_empty() {
        return Some(message);
    }
    let again = format!(
        "{}\n\nYour last message, below, broke these rules: {}\n\n{message}",
        writer.prompt,
        problems.join(" ")
    );
    let retry = this
        .update(cx, |this, cx| writer.ask(this, again, cx))
        .ok()?;
    Some(
        retry
            .await
            .map_or(message, |written| writer.rules.clean(&written)),
    )
}

/// The model's instructions, with the project's rules in them.
fn instructions(rules: &MessageRules) -> String {
    let mut text = String::from(
        "You write git commit messages. Answer with the message alone, no Markdown and no \
         quotes: a first line in the imperative mood that says what changed, then, if it \
         helps, a blank line and a few lines on why. Do not credit anyone.",
    );
    if rules.subject_max > 0 {
        let _ = write!(
            text,
            " The first line has at most {} characters.",
            rules.subject_max
        );
    }
    if rules.conventional {
        text.push_str(
            " The first line reads `type(scope): subject`, such as `fix(parser): stop at the end`.",
        );
    }
    text
}

fn prompt(asked: &str, answered: &str, diff: &str) -> String {
    format!(
        "The user asked an agent:\n{asked}\n\nThe agent answered:\n{answered}\n\nWrite the commit message for these changes:\n\n{diff}"
    )
}
