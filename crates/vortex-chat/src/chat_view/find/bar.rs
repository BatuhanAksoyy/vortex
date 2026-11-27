//! The search bar over the transcript: the query, how many matches it has,
//! whether tool output is searched, and the way through the matches.

use gpui_kit::prelude::*;
use gpui_kit::{
    Context, KeyDownEvent, SharedString, TestSupportExt, Window,
    assets::IconName as Lucide,
    base::Selectable as _,
    component::{
        ActiveTheme, Disableable as _, IconName, Sizable as _,
        button::{Button, ButtonVariants as _},
        h_flex,
        input::Input,
    },
    div, rems,
};

use super::super::ChatView;
use crate::scale::{ScaledButton as _, ScaledText as _};

const INPUT_REMS: f32 = 16.0;

impl ChatView {
    pub(in crate::chat_view) fn render_find(&self, cx: &Context<Self>) -> Option<impl IntoElement> {
        let find = self.find.as_ref()?;
        let theme = cx.theme();
        let searched = !find.input.read(cx).value().trim().is_empty();
        let count = match (find.current, find.matches.len()) {
            (_, 0) if searched => "No results".to_owned(),
            (_, 0) => String::new(),
            (Some(current), total) => format!("{} of {total}", current + 1),
            (None, total) => format!("{total} found"),
        };
        let none = find.matches.is_empty();
        Some(
            h_flex()
                .id("chat-find")
                .test_support()
                .on_key_down(cx.listener(Self::on_find_key))
                .gap_1()
                .px_3()
                .py_1()
                .border_b_1()
                .border_color(theme.border)
                .child(
                    div()
                        .id("chat-find-input")
                        .test_support()
                        .w(rems(INPUT_REMS))
                        .child(Input::new(&find.input).small().cleanable(true)),
                )
                .child(
                    div()
                        .id("chat-find-count")
                        .test_support()
                        .aria_label(SharedString::from(count.clone()))
                        .flex_1()
                        .px_1()
                        .text_caption()
                        .text_color(theme.muted_foreground)
                        .child(count),
                )
                .children(find_buttons(find.tool_output, none, cx)),
        )
    }

    fn on_find_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key == "escape" {
            self.close_find(window, cx);
            cx.stop_propagation();
        }
    }
}

/// Whether tool output is searched, the way through the matches, and
/// closing; `none` when nothing matches.
fn find_buttons(tool_output: bool, none: bool, cx: &Context<ChatView>) -> Vec<Button> {
    let button = |id: &'static str, tooltip: &'static str| Button::new(id).ghost().tooltip(tooltip);
    vec![
        button("chat-find-output", "Search tool output")
            .row_icon(Lucide::SquareTerminal)
            .selected(tool_output)
            .on_click(cx.listener(|this, _, _, cx| this.toggle_find_output(cx))),
        button("chat-find-previous", "Previous match (Shift+Enter)")
            .row_icon(Lucide::ChevronUp)
            .disabled(none)
            .on_click(cx.listener(|this, _, _, cx| this.find_previous(cx))),
        button("chat-find-next", "Next match (Enter)")
            .row_icon(Lucide::ChevronDown)
            .disabled(none)
            .on_click(cx.listener(|this, _, _, cx| this.find_next(cx))),
        button("chat-find-close", "Close (Escape)")
            .row_icon(IconName::Close)
            .on_click(cx.listener(|this, _, window, cx| this.close_find(window, cx))),
    ]
}
