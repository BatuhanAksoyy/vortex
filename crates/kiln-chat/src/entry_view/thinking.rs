//! A thought of the model's: a line saying how long it thought and about
//! how many tokens it took, which opens to the summary of what it thought.

use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, App, TestSupportExt, component::ActiveTheme, div};

use crate::clock;

use super::{EntryContext, timeline_row, toggle};

pub(super) fn render(
    text: &str,
    tokens: u64,
    took: Option<Duration>,
    context: &EntryContext,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let has_words = !text.trim().is_empty();
    let expanded = context.toggled && has_words;
    let arrow = match (has_words, expanded) {
        (false, _) => "",
        (true, true) => " ▾",
        (true, false) => " ▸",
    };
    let line = div()
        .id(("thinking", context.id))
        .test_support()
        .italic()
        .text_color(muted)
        .child(format!("{}{arrow}", label(tokens, took)))
        .when(has_words, |line| {
            line.cursor_pointer().on_click(toggle(context))
        });
    timeline_row(muted, context, cx)
        .child(line)
        .when(expanded, |this| {
            this.child(div().italic().text_color(muted).child(text.to_owned()))
        })
        .into_any_element()
}

/// `Thought for 4.2s · ~310 tokens`; the tokens are the CLI's estimate.
fn label(tokens: u64, took: Option<Duration>) -> String {
    let time = took.map_or_else(
        || "Thinking".to_owned(),
        |took| format!("Thought for {}", clock::took(took)),
    );
    if tokens == 0 {
        return time;
    }
    format!("{time} · ~{tokens} tokens")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_thought_says_how_long_and_how_much() {
        assert_eq!(label(0, None), "Thinking");
        assert_eq!(label(214, None), "Thinking · ~214 tokens");
        assert_eq!(
            label(214, Some(Duration::from_millis(1154))),
            "Thought for 1.2s · ~214 tokens"
        );
        assert_eq!(
            label(0, Some(Duration::from_secs(75))),
            "Thought for 1m 15s"
        );
    }
}
