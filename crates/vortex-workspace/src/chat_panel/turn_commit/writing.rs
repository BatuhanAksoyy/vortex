//! The writer model's part: it judges whether a turn finished the work,
//! then writes the commit message by the project's rules and directives.
//! It is asked once more when the message breaks a rule.

use std::fmt::Write as _;

use gpui_kit::{App, AsyncWindowContext, WeakEntity};
use vortex_git::MessageRules;
use vortex_settings::CommitSettings;

use super::super::ChatPanel;
use crate::commit_review;

/// What the writer answers when the turn did not finish the work.
const NOT_DONE: &str = "NOT_DONE";

/// What came back from the writer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Written {
    Message(String),
    /// The turn did not finish what it was asked; nothing is committed.
    NotDone,
}

/// How the message is asked for.
pub(super) struct Writer {
    agent: String,
    model: String,
    system: String,
    rules: MessageRules,
    judges: bool,
}

impl Writer {
    /// The writer the settings name; `judges` has it first decide whether
    /// the turn finished the work.
    ///
    /// # Errors
    /// When one of the settings' patterns is not a valid regular expression.
    pub(super) fn new(commit: &CommitSettings, judges: bool) -> Result<Self, String> {
        let rules = commit_review::message_rules(commit)?;
        Ok(Self {
            agent: commit.writer.agent.clone(),
            model: commit.writer.model.clone(),
            system: instructions(&rules, &commit.instructions, judges),
            rules,
            judges,
        })
    }

    fn ask(&self, panel: &ChatPanel, prompt: String, cx: &App) -> vortex_agent::Pending<String> {
        let question = vortex_chat::Question {
            agent: self.agent.clone(),
            model: self.model.clone(),
            system: self.system.clone(),
            prompt,
        };
        panel.chat.read(cx).ask_once(question, cx)
    }

    /// The writer's answer, its message cleaned; asked once more if the
    /// message breaks the rules. `None` when the writer could not answer.
    pub(super) async fn write(
        &self,
        this: &WeakEntity<ChatPanel>,
        prompt: String,
        cx: &mut AsyncWindowContext,
    ) -> Option<Written> {
        let first = this.update(cx, |this, cx| self.ask(this, prompt.clone(), cx));
        let answer = match first.ok()?.await {
            Ok(answer) => answer,
            Err(error) => {
                log::warn!("the commit writer did not answer: {error:#}");
                return None;
            }
        };
        if self.judges && answer.trim() == NOT_DONE {
            return Some(Written::NotDone);
        }
        let message = self.rules.clean(&answer);
        let problems = self.rules.problems(&message);
        if problems.is_empty() {
            return Some(Written::Message(message));
        }
        let again = format!(
            "{prompt}\n\nYour last message, below, broke these rules: {}\n\n{message}",
            problems.join(" ")
        );
        let retry = this.update(cx, |this, cx| self.ask(this, again, cx)).ok()?;
        let message = retry
            .await
            .map_or(message, |written| self.rules.clean(&written));
        Some(Written::Message(message))
    }
}

/// The writer's instructions, with the project's rules and directives.
fn instructions(rules: &MessageRules, directives: &[String], judges: bool) -> String {
    let mut text = String::new();
    if judges {
        let _ = write!(
            text,
            "You decide whether an agent finished the work the user asked for. It has not when \
             it asks the user a question or for a choice, says that work remains, or stopped \
             partway. If it has not, answer with the single word {NOT_DONE} and nothing else. \
             If it has, "
        );
    }
    text.push_str(
        "write the git commit message for its changes. Answer with the message alone, no \
         Markdown and no quotes: a first line in the imperative mood that says what changed, \
         then, if it helps, a blank line and a few lines on why. Do not credit anyone.",
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
    for pattern in &rules.require {
        let _ = write!(
            text,
            " The message must match the regular expression `{pattern}`."
        );
    }
    for pattern in &rules.forbid {
        let _ = write!(
            text,
            " The message must not match the regular expression `{pattern}`."
        );
    }
    if !directives.is_empty() {
        text.push_str("\n\nThe project asks this of every commit message:");
        for directive in directives {
            let _ = write!(text, "\n- {directive}");
        }
    }
    text
}

pub(super) fn prompt(asked: &str, answered: &str, diff: &str) -> String {
    format!(
        "The user asked an agent:\n{asked}\n\nThe agent answered:\n{answered}\n\nIts changes:\n\n{diff}"
    )
}

#[cfg(test)]
mod tests {
    use vortex_git::MessageRules;

    use super::instructions;

    #[test]
    fn the_directives_and_checks_reach_the_writer() {
        let rules = MessageRules::new(&[], 50, false)
            .and_then(|rules| rules.with_checks(&["^KLN-".to_owned()], &["(?i)wip".to_owned()]))
            .expect("valid");
        let told = instructions(&rules, &["Write in English.".to_owned()], true);
        assert!(told.contains("NOT_DONE"), "{told}");
        assert!(told.contains("at most 50"), "{told}");
        assert!(
            told.contains("`^KLN-`") && told.contains("`(?i)wip`"),
            "{told}"
        );
        assert!(told.ends_with("- Write in English."), "{told}");
        assert!(!instructions(&rules, &[], false).contains("NOT_DONE"));
    }
}
