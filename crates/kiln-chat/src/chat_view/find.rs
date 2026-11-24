//! Finding text in the conversation, opened with Ctrl+F (Cmd+F on macOS):
//! every match counted, the current one scrolled to, and Enter and
//! Shift+Enter going through them. Messages and the agent's text show their
//! matches highlighted; other rows that match, such as tool calls, are
//! marked whole.

mod bar;
mod matches;

use std::{collections::HashMap, ops::Range};

use gpui_kit::{
    AppContext as _, Context, Entity, Focusable as _, Hsla, Subscription, Window,
    component::{
        ActiveTheme,
        input::{InputEvent, InputState},
        text::{RangeHighlight, TextViewState},
    },
};

use super::ChatView;
use crate::{entry_view, transcript::EntryId};

pub(super) struct Find {
    input: Entity<InputState>,
    /// Whether tool output is searched too; it is long, and seldom what is
    /// looked for.
    tool_output: bool,
    /// In transcript order.
    matches: Vec<Match>,
    current: Option<usize>,
    /// The entries whose matches are highlighted in their text.
    texts: HashMap<EntryId, Highlighted>,
    _input: Subscription,
}

/// A text that shows its matches, drawn from a state of its own while the
/// search lasts.
struct Highlighted {
    source: String,
    state: Entity<TextViewState>,
    /// The text last searched. Setting highlights notifies the state too, so
    /// only a new parse starts another search.
    searched: Option<String>,
    _parsed: Subscription,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Match {
    entry: EntryId,
    /// Where it is in a highlighted text; `None` marks the whole row.
    range: Option<Range<usize>>,
}

impl ChatView {
    /// Shows the search, or focuses it again.
    pub fn open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.find.is_none() {
            let input =
                cx.new(|cx| InputState::new(window, cx).placeholder("Find in conversation"));
            let subscription = cx.subscribe(&input, Self::on_find_input);
            self.find = Some(Find {
                input,
                tool_output: false,
                matches: Vec::new(),
                current: None,
                texts: HashMap::new(),
                _input: subscription,
            });
        }
        let Some(input) = self.find.as_ref().map(|find| find.input.clone()) else {
            return;
        };
        let focus = input.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        cx.notify();
    }

    pub(super) fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.find.take().is_some() {
            let composer = self.composer.read(cx).focus_handle(cx);
            window.focus(&composer, cx);
            cx.notify();
        }
    }

    fn on_find_input(&mut self, _: Entity<InputState>, event: &InputEvent, cx: &mut Context<Self>) {
        match event {
            InputEvent::Change => {
                if let Some(find) = &mut self.find {
                    find.current = None;
                }
                self.refresh_find(true, cx);
            }
            InputEvent::PressEnter { shift, .. } => self.step_find(!shift, cx),
            InputEvent::Focus | InputEvent::Blur => {}
        }
    }

    pub(super) fn toggle_find_output(&mut self, cx: &mut Context<Self>) {
        if let Some(find) = &mut self.find {
            find.tool_output = !find.tool_output;
            self.refresh_find(true, cx);
        }
    }

    /// Searches the transcript again, after the query, the transcript or a
    /// highlighted text's parse changed; scrolls to the current match when
    /// `follow` is set.
    pub(super) fn refresh_find(&mut self, follow: bool, cx: &mut Context<Self>) {
        let Some(find) = &mut self.find else {
            return;
        };
        let query = find.input.read(cx).value().trim().to_owned();
        let tool_output = find.tool_output;
        let mut old = std::mem::take(&mut find.texts);
        let rows = self.rows_matching(&query, tool_output);
        let mut found = Vec::new();
        let mut texts = HashMap::new();
        for (entry, markdown) in rows {
            let Some(source) = markdown else {
                found.push(Match { entry, range: None });
                continue;
            };
            let mut text = highlighted(&mut old, entry, source, cx);
            let rendered = text.state.read(cx).rendered_text().as_str().to_owned();
            let ranges = matches::occurrences(&rendered, &query);
            // Until its parse lands, or when the match is in what the
            // formatting leaves out, the row is marked whole.
            if ranges.is_empty() {
                found.push(Match { entry, range: None });
            }
            found.extend(ranges.into_iter().map(|range| Match {
                entry,
                range: Some(range),
            }));
            text.searched = Some(rendered);
            texts.insert(entry, text);
        }
        let top = self.first_shown_entry(cx);
        let Some(find) = &mut self.find else {
            return;
        };
        let previous = find.current.and_then(|current| find.matches.get(current));
        let current = previous
            .and_then(|previous| found.iter().position(|found| found == previous))
            .or_else(|| first_from(&found, top, &self.transcript))
            .or_else(|| found.len().checked_sub(1));
        find.matches = found;
        find.texts = texts;
        find.current = current;
        if follow {
            self.reveal_current(cx);
        }
        self.paint_find(cx);
        cx.notify();
    }

    /// Each entry with a match: the formatted text to highlight it in, or
    /// `None` to mark the row whole, as for a tool call or a long message
    /// shown in part.
    fn rows_matching(&self, query: &str, tool_output: bool) -> Vec<(EntryId, Option<String>)> {
        if query.is_empty() {
            return Vec::new();
        }
        (0..self.transcript.len())
            .filter_map(|row| {
                let (id, entry) = self.transcript.entry(row)?;
                let plain = matches::plain_text(entry, tool_output, &self.cwd);
                if matches::occurrences(&plain, query).is_empty() {
                    return None;
                }
                Some((
                    id,
                    entry_view::markdown_source(entry, self.toggled.contains(&id)),
                ))
            })
            .collect()
    }

    /// The entry at the top of the transcript's view.
    fn first_shown_entry(&self, cx: &gpui_kit::App) -> Option<EntryId> {
        let row = self.scroller.read(cx).logical_scroll_top().item_ix;
        self.transcript.entry(row).map(|(id, _)| id)
    }

    /// Moves to the next match, or the previous one, round the ends.
    fn step_find(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some(find) = &mut self.find else {
            return;
        };
        let count = find.matches.len();
        if count == 0 {
            return;
        }
        find.current = Some(match (find.current, forward) {
            (None, _) => 0,
            (Some(current), true) => (current + 1) % count,
            (Some(current), false) => (current + count - 1) % count,
        });
        self.reveal_current(cx);
        self.paint_find(cx);
        cx.notify();
    }

    pub(super) fn find_previous(&mut self, cx: &mut Context<Self>) {
        self.step_find(false, cx);
    }

    pub(super) fn find_next(&mut self, cx: &mut Context<Self>) {
        self.step_find(true, cx);
    }

    /// Scrolls the current match's row into view, then the match within it.
    fn reveal_current(&self, cx: &mut Context<Self>) {
        let Some((found, text)) = self.current_match() else {
            return;
        };
        let Some(row) = row_of(&self.transcript, found.entry) else {
            return;
        };
        self.scroller
            .update(cx, |scroller, cx| scroller.scroll_to_item(row, cx));
        let (Some(range), Some(text)) = (found.range.clone(), text) else {
            return;
        };
        let revealed = text.update(cx, |text, cx| text.reveal_range(range, cx));
        if let Err(error) = revealed {
            log::debug!("cannot scroll to the match: {error}");
        }
    }

    fn current_match(&self) -> Option<(&Match, Option<&Entity<TextViewState>>)> {
        let find = self.find.as_ref()?;
        let found = find.matches.get(find.current?)?;
        let text = find.texts.get(&found.entry).map(|text| &text.state);
        Some((found, text))
    }

    /// Sets each highlighted text's matches, the current one brighter.
    fn paint_find(&self, cx: &mut Context<Self>) {
        let Some(find) = &self.find else {
            return;
        };
        let colors = colors(cx);
        for (entry, text) in &find.texts {
            let highlights = find.highlights(*entry, colors);
            let set = text
                .state
                .update(cx, |state, cx| state.set_range_highlights(highlights, cx));
            if let Err(error) = set {
                log::debug!("cannot highlight the matches: {error}");
            }
        }
    }

    /// A text's parse landed: its matches are searched again.
    fn on_text_parsed(&mut self, entry: EntryId, cx: &mut Context<Self>) {
        let Some(text) = self.find.as_ref().and_then(|find| find.texts.get(&entry)) else {
            return;
        };
        let rendered = text.state.read(cx).rendered_text();
        if text.searched.as_deref() != Some(rendered.as_str()) {
            self.refresh_find(false, cx);
        }
    }

    /// The state `entry`'s text draws from while it shows matches.
    pub(super) fn found_text(&self, entry: EntryId) -> Option<&Entity<TextViewState>> {
        let find = self.find.as_ref()?;
        find.texts.get(&entry).map(|text| &text.state)
    }

    /// The colour `entry` is marked in when it is marked whole as a match,
    /// brighter for the current one.
    pub(super) fn row_mark(&self, entry: EntryId, cx: &gpui_kit::App) -> Option<Hsla> {
        let find = self.find.as_ref()?;
        let current = find.current.and_then(|current| find.matches.get(current));
        let row_match = Match { entry, range: None };
        let (match_color, current_color) = colors(cx);
        find.matches.contains(&row_match).then(|| {
            if current == Some(&row_match) {
                current_color
            } else {
                match_color
            }
        })
    }
}

impl Find {
    /// `entry`'s matches in its text, in the match colour or, for the
    /// current one, the current colour.
    fn highlights(&self, entry: EntryId, (normal, current): (Hsla, Hsla)) -> Vec<RangeHighlight> {
        let shown = self.current.and_then(|index| self.matches.get(index));
        self.matches
            .iter()
            .filter(|found| found.entry == entry)
            .filter_map(|found| {
                let color = if Some(found) == shown {
                    current
                } else {
                    normal
                };
                Some(RangeHighlight::new(found.range.clone()?, color))
            })
            .collect()
    }
}

/// `entry`'s highlighted text, kept from the last search when it still
/// shows `source`.
fn highlighted(
    old: &mut HashMap<EntryId, Highlighted>,
    entry: EntryId,
    source: String,
    cx: &mut Context<ChatView>,
) -> Highlighted {
    if let Some(mut text) = old.remove(&entry) {
        if text.source != source {
            text.state
                .update(cx, |state, cx| state.set_text(&source, cx));
            text.source = source;
        }
        return text;
    }
    let state = cx.new(|cx| TextViewState::markdown(&source, cx));
    let parsed = cx.observe(&state, move |view, _, cx| view.on_text_parsed(entry, cx));
    Highlighted {
        source,
        state,
        searched: None,
        _parsed: parsed,
    }
}

/// The first match at or below the entry at the top of the view.
fn first_from(
    found: &[Match],
    top: Option<EntryId>,
    transcript: &crate::transcript::Transcript,
) -> Option<usize> {
    let top = row_of(transcript, top?)?;
    found
        .iter()
        .position(|found| row_of(transcript, found.entry).is_some_and(|row| row >= top))
}

fn row_of(transcript: &crate::transcript::Transcript, entry: EntryId) -> Option<usize> {
    (0..transcript.len()).find(|row| transcript.entry(*row).is_some_and(|(id, _)| id == entry))
}

/// The background of a match, and of the current one.
fn colors(cx: &gpui_kit::App) -> (Hsla, Hsla) {
    let warning = cx.theme().warning;
    (warning.opacity(0.25), warning.opacity(0.6))
}
