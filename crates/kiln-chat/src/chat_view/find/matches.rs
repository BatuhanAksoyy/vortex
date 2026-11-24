//! Where a query occurs in a text, ignoring case, and what of each
//! transcript entry a search reads.

use std::{ops::Range, path::Path};

use kiln_agent::ToolCall;

use crate::{
    tool_format,
    transcript::{Entry, ToolResult},
};

/// The byte ranges of `query` in `text`, ignoring case, without overlaps.
/// Each range covers whole characters of `text`, whose lowercase may be
/// longer or shorter than the character itself.
pub fn occurrences(text: &str, query: &str) -> Vec<Range<usize>> {
    let needle: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();
    if needle.is_empty() {
        return Vec::new();
    }
    let mut found = Vec::new();
    let mut from = 0;
    for (start, _) in text.char_indices() {
        if start < from {
            continue;
        }
        if let Some(end) = match_at(text, start, &needle) {
            found.push(start..end);
            from = end;
        }
    }
    found
}

/// The end of a match of `needle` that starts at `start`.
fn match_at(text: &str, start: usize, needle: &[char]) -> Option<usize> {
    let mut rest = needle;
    for (offset, char) in text[start..].char_indices() {
        for lower in char.to_lowercase() {
            let (first, tail) = rest.split_first()?;
            if *first != lower {
                return None;
            }
            rest = tail;
        }
        if rest.is_empty() {
            return Some(start + offset + char.len_utf8());
        }
    }
    None
}

/// All of `entry` a search reads: a message's whole text, also the part a
/// long one hides, a tool's name and what it was called on, and its output
/// when `tool_output` is set.
pub fn plain_text(entry: &Entry, tool_output: bool, root: &Path) -> String {
    match entry {
        Entry::User(message) => message.text.clone(),
        Entry::Text { text, .. } | Entry::Thinking { text, .. } | Entry::Notice { text, .. } => {
            text.clone()
        }
        Entry::Tool { call, result } => tool_text(call, result.as_ref(), tool_output, root),
        Entry::Subagent { call, agent, .. } => {
            let mut text = format!(
                "{} {}",
                agent.description,
                tool_text(call, None, false, root)
            );
            if let Some(summary) = &agent.summary {
                text.push('\n');
                text.push_str(summary);
            }
            text
        }
    }
}

fn tool_text(call: &ToolCall, result: Option<&ToolResult>, output: bool, root: &Path) -> String {
    let mut text = format!("{} {}", call.name, tool_format::detail(&call.input, root));
    if let Some(result) = result.filter(|_| output) {
        text.push('\n');
        text.push_str(&result.output);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::occurrences;

    #[test]
    fn occurrences_ignore_case_and_do_not_overlap() {
        assert_eq!(
            occurrences("Fix the build, then build", "BUILD"),
            [8..13, 20..25]
        );
        assert_eq!(occurrences("aaaa", "aa"), [0..2, 2..4]);
        assert_eq!(occurrences("anything", "").len(), 0);
        assert_eq!(occurrences("short", "shorter").len(), 0);
    }

    #[test]
    fn ranges_cover_whole_characters_of_the_text() {
        // `İ` lowercases to two characters, `i` and a combining dot.
        let text = "İstanbul ve Çanakkale";
        assert_eq!(occurrences(text, "i̇stanbul").first(), Some(&(0..9)));
        assert_eq!(occurrences(text, "çanak").first(), Some(&(13..19)));
        assert_eq!(&text[13..19], "Çanak");
    }
}
