//! What a commit message must look like: lines that credit a tool are
//! taken out, and the rest is checked against the project's rules. Both
//! are plain text rules, so they hold whoever wrote the message.

use regex::Regex;

/// The rules a message is held to.
#[derive(Clone, Debug)]
pub struct MessageRules {
    /// Lines matching any of these are taken out.
    pub strip: Vec<Regex>,
    /// The first line's most characters; 0 for no limit.
    pub subject_max: usize,
    /// The first line must read `type(scope): subject`.
    pub conventional: bool,
    /// The message must match each of these.
    pub require: Vec<Regex>,
    /// The message must match none of these.
    pub forbid: Vec<Regex>,
}

fn patterns(patterns: &[String]) -> Result<Vec<Regex>, String> {
    patterns
        .iter()
        .map(|pattern| Regex::new(pattern).map_err(|error| format!("`{pattern}`: {error}")))
        .collect()
}

impl MessageRules {
    /// Rules from the settings' patterns, or the pattern that is wrong.
    ///
    /// # Errors
    /// When a pattern is not a valid regular expression.
    pub fn new(strip: &[String], subject_max: usize, conventional: bool) -> Result<Self, String> {
        Ok(Self {
            strip: patterns(strip)?,
            subject_max,
            conventional,
            require: Vec::new(),
            forbid: Vec::new(),
        })
    }

    /// The rules, also holding a message to match each of `require` and
    /// none of `forbid`.
    ///
    /// # Errors
    /// When a pattern is not a valid regular expression.
    pub fn with_checks(self, require: &[String], forbid: &[String]) -> Result<Self, String> {
        Ok(Self {
            require: patterns(require)?,
            forbid: patterns(forbid)?,
            ..self
        })
    }

    /// `message` without the lines the rules take out, without trailing
    /// spaces, and with no more than one blank line in a row.
    #[must_use]
    pub fn clean(&self, message: &str) -> String {
        let mut lines: Vec<&str> = Vec::new();
        for line in message.lines().map(str::trim_end) {
            if self.strip.iter().any(|pattern| pattern.is_match(line)) {
                continue;
            }
            if line.is_empty() && lines.last().is_none_or(|last| last.is_empty()) {
                continue;
            }
            lines.push(line);
        }
        while lines.last().is_some_and(|line| line.is_empty()) {
            lines.pop();
        }
        lines.join("\n")
    }

    /// What is wrong with an already cleaned `message`, each said in a
    /// sentence; empty when it may be committed.
    #[must_use]
    pub fn problems(&self, message: &str) -> Vec<String> {
        let Some(subject) = message
            .lines()
            .next()
            .filter(|line| !line.trim().is_empty())
        else {
            return vec!["The message is empty.".to_owned()];
        };
        let mut problems = Vec::new();
        let length = subject.chars().count();
        if self.subject_max > 0 && length > self.subject_max {
            problems.push(format!(
                "The first line has {length} characters; at most {} are allowed.",
                self.subject_max
            ));
        }
        if self.conventional && !conventional(subject) {
            problems.push(
                "The first line must read `type(scope): subject`, such as `fix: handle empty input`."
                    .to_owned(),
            );
        }
        if message
            .lines()
            .nth(1)
            .is_some_and(|line| !line.trim().is_empty())
        {
            problems.push("A blank line must follow the first line.".to_owned());
        }
        for pattern in self.require.iter().filter(|it| !it.is_match(message)) {
            problems.push(format!("The message must match `{pattern}`."));
        }
        for pattern in self.forbid.iter().filter(|it| it.is_match(message)) {
            problems.push(format!("The message must not match `{pattern}`."));
        }
        problems
    }
}

/// `feat: …`, `fix(parser)!: …`: a lowercase type, an optional scope and
/// `!`, then a colon and a subject.
fn conventional(subject: &str) -> bool {
    let Some((head, rest)) = subject.split_once(": ") else {
        return false;
    };
    let head = head.strip_suffix('!').unwrap_or(head);
    let (kind, scope) = match head.split_once('(') {
        Some((kind, scope)) => (kind, scope.strip_suffix(')')),
        None => (head, Some("")),
    };
    let lowercase = |text: &str| !text.is_empty() && text.chars().all(|c| c.is_ascii_lowercase());
    lowercase(kind)
        && scope.is_some_and(|scope| !scope.contains(['(', ')']))
        && !rest.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::MessageRules;

    fn rules(subject_max: usize, conventional: bool) -> MessageRules {
        let strip = [
            r"(?i)^co-authored-by:.*(claude|codex|openai|anthropic)".to_owned(),
            r"Generated with \[Claude Code\]".to_owned(),
        ];
        MessageRules::new(&strip, subject_max, conventional).expect("valid")
    }

    #[test]
    fn credit_lines_are_taken_out() {
        let message = "Fix the parser  \n\nIt read past the end.\n\n\n🤖 Generated with [Claude Code](https://claude.com)\n\nCo-Authored-By: Claude <noreply@anthropic.com>\nCo-Authored-By: Ada <ada@example.com>\n";
        assert_eq!(
            rules(0, false).clean(message),
            "Fix the parser\n\nIt read past the end.\n\nCo-Authored-By: Ada <ada@example.com>"
        );
    }

    #[test]
    fn the_rules_say_what_is_wrong() {
        let strict = rules(20, true);
        assert!(
            strict.problems("fix(parser): stop early").len() == 1,
            "too long"
        );
        assert_eq!(strict.problems("feat!: drop v1"), Vec::<String>::new());
        assert_eq!(strict.problems("Fix it").len(), 1, "not conventional");
        assert_eq!(strict.problems("").len(), 1);
        assert_eq!(
            rules(0, false).problems("Fix\nmore").len(),
            1,
            "no blank line"
        );
        assert!(MessageRules::new(&["(".to_owned()], 0, false).is_err());
    }

    #[test]
    fn a_message_must_match_what_is_required_and_not_what_is_forbidden() {
        let checked = rules(0, false)
            .with_checks(&[r"^[A-Z]+-\d+ ".to_owned()], &["(?i)wip".to_owned()])
            .expect("valid");
        assert_eq!(
            checked.problems("KLN-12 Fix the parser"),
            Vec::<String>::new()
        );
        assert_eq!(checked.problems("Fix the parser").len(), 1, "no ticket");
        assert_eq!(checked.problems("KLN-12 WIP parser").len(), 1, "forbidden");
        assert!(rules(0, false).with_checks(&[], &["(".to_owned()]).is_err());
    }
}
