//! What the page's console says, under the page.

use std::collections::VecDeque;

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, Context, ScrollHandle, TestSupportExt,
    base::Selectable as _,
    component::{
        ActiveTheme, Disableable as _, Sizable as _,
        button::{Button, ButtonVariants as _},
        h_flex, v_flex,
    },
    div,
};
use vortex_browser::{ConsoleEntry, Level};
use vortex_chat::scale::ScaledText as _;

use super::Browser;

/// The newest ones; a page can write any number.
const MOST: usize = 300;

#[derive(Default)]
pub(super) struct Console {
    lines: VecDeque<Line>,
    errors_only: bool,
    scroll: ScrollHandle,
}

struct Line {
    entry: ConsoleEntry,
    /// How many times in a row it was written.
    times: usize,
}

impl Console {
    pub fn push(&mut self, entry: ConsoleEntry) {
        if let Some(last) = self.lines.back_mut().filter(|last| last.entry == entry) {
            last.times += 1;
            return;
        }
        if self.lines.len() == MOST {
            self.lines.pop_front();
        }
        self.lines.push_back(Line { entry, times: 1 });
        self.scroll.scroll_to_bottom();
    }

    pub fn clear(&mut self) {
        self.lines.clear();
    }

    pub fn entries(&self) -> impl Iterator<Item = &ConsoleEntry> {
        self.lines.iter().map(|line| &line.entry)
    }

    pub fn errors(&self) -> impl Iterator<Item = &ConsoleEntry> {
        let entries = self.lines.iter().map(|line| &line.entry);
        entries.filter(|entry| entry.level == Level::Error)
    }
}

impl Browser {
    pub(super) fn render_console(&self, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let shown = self.console.lines.iter();
        let errors_only = self.console.errors_only;
        let shown = shown.filter(|line| !errors_only || line.entry.level == Level::Error);
        let lines: Vec<_> = shown.map(|line| render_line(line, cx)).collect();
        let list = v_flex()
            .id("browser-console-lines")
            .test_support()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.console.scroll)
            .font_family(theme.mono_font_family.clone())
            .text_caption()
            .when(lines.is_empty(), |list| list.child(empty(errors_only, cx)))
            .children(lines);
        list.into_any_element()
    }

    pub(super) fn console_actions(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let no_errors = self.console.errors().next().is_none();
        h_flex()
            .gap_1()
            .child(
                Button::new("browser-console-errors")
                    .ghost()
                    .small()
                    .label("Errors Only")
                    .selected(self.console.errors_only)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.console.errors_only = !this.console.errors_only;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("browser-console-attach")
                    .ghost()
                    .small()
                    .label("Attach Errors")
                    .tooltip("Attach the errors to the message")
                    .disabled(no_errors)
                    .on_click(cx.listener(|this, _, _, cx| this.attach_errors(cx))),
            )
            .child(
                Button::new("browser-console-clear")
                    .ghost()
                    .small()
                    .label("Clear")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.console.clear();
                        cx.notify();
                    })),
            )
    }
}

fn empty(errors_only: bool, cx: &Context<Browser>) -> AnyElement {
    let text = if errors_only {
        "No errors"
    } else {
        "Nothing written yet"
    };
    div()
        .p_2()
        .text_color(cx.theme().muted_foreground)
        .child(text)
        .into_any_element()
}

fn render_line(line: &Line, cx: &Context<Browser>) -> AnyElement {
    let theme = cx.theme();
    let color = match line.entry.level {
        Level::Error => theme.danger,
        Level::Warning => theme.warning,
        Level::Info => theme.foreground,
    };
    let times = (line.times > 1).then(|| format!("×{}", line.times));
    let place = line.entry.place.clone();
    let muted = |text: String| {
        div()
            .flex_none()
            .text_color(theme.muted_foreground)
            .child(text)
    };
    h_flex()
        .flex_none()
        .items_start()
        .gap_2()
        .px_2()
        .py_0p5()
        .border_b_1()
        .border_color(theme.border)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_color(color)
                .child(line.entry.text.clone()),
        )
        .children(times.map(muted))
        .children(place.map(|place| muted(short(&place).to_owned())))
        .into_any_element()
}

/// The last part of a place, which is the file and line.
fn short(place: &str) -> &str {
    let last = place.trim_end_matches('/').rsplit('/').next();
    last.unwrap_or(place)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(level: Level, text: &str) -> ConsoleEntry {
        ConsoleEntry {
            level,
            text: text.to_owned(),
            place: None,
        }
    }

    #[test]
    fn repeats_are_counted_and_old_lines_dropped() {
        let mut console = Console::default();
        console.push(entry(Level::Error, "x"));
        console.push(entry(Level::Error, "x"));
        assert_eq!(console.lines.len(), 1);
        assert_eq!(console.lines[0].times, 2);
        for number in 0..MOST {
            console.push(entry(Level::Info, &number.to_string()));
        }
        assert_eq!(console.lines.len(), MOST);
        assert_eq!(console.errors().count(), 0);
    }

    #[test]
    fn a_place_is_shortened_to_its_file() {
        assert_eq!(short("http://localhost:3000/src/app.js:10"), "app.js:10");
        assert_eq!(short("http://localhost:3000/"), "localhost:3000");
    }
}
