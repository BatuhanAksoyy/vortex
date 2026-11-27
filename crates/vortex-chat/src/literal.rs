//! Plain text as Markdown that renders it as written, so a message can be
//! shown in a selectable text view without its `*`, `#` or `1.` turning
//! into formatting.

/// `text` with every ASCII punctuation mark escaped, each line break kept
/// as a hard break, and runs of spaces kept as no-break spaces, which
/// Markdown would otherwise fold into one.
pub fn literal_markdown(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = String::with_capacity(text.len() * 2);
    for (index, line) in lines.iter().enumerate() {
        escape_line(line, &mut out);
        let next_is_text = lines
            .get(index + 1)
            .is_some_and(|next| !next.trim().is_empty());
        if index + 1 < lines.len() {
            // A blank line on either side is a paragraph break already.
            if !line.trim().is_empty() && next_is_text {
                out.push('\\');
            }
            out.push('\n');
        }
    }
    out
}

fn escape_line(line: &str, out: &mut String) {
    let mut after_space = true;
    for ch in line.chars() {
        match ch {
            ' ' if after_space => out.push('\u{a0}'),
            ' ' => out.push(' '),
            '\t' => out.push_str("\u{a0}\u{a0}\u{a0}\u{a0}"),
            ch if ch.is_ascii_punctuation() => {
                out.push('\\');
                out.push(ch);
            }
            ch => out.push(ch),
        }
        after_space = matches!(ch, ' ' | '\t');
    }
}

#[cfg(test)]
mod tests {
    use super::literal_markdown;

    #[test]
    fn markup_is_escaped() {
        assert_eq!(literal_markdown("# not a *title*"), r"\# not a \*title\*");
        assert_eq!(literal_markdown("1. one"), r"1\. one");
    }

    #[test]
    fn line_breaks_are_kept_and_blank_lines_stay_paragraph_breaks() {
        assert_eq!(literal_markdown("one\ntwo"), "one\\\ntwo");
        assert_eq!(literal_markdown("one\n\ntwo"), "one\n\ntwo");
    }

    #[test]
    fn indentation_and_runs_of_spaces_survive() {
        assert_eq!(
            literal_markdown("    code  here"),
            "\u{a0}\u{a0}\u{a0}\u{a0}code \u{a0}here"
        );
    }
}
