//! The chat panel's layout, after the Claude Code VS Code panel: a title
//! bar (in `header`), the transcript timeline, and the composer (in
//! `composer`) or the permission prompt in its place.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, Context, StyleRefinement, TestSupportExt, Window,
    component::{ActiveTheme, message_scroller::MessageScroller, v_flex},
    div, px,
};

use super::ChatView;
use crate::{
    entry_view::{self, EntryContext},
    spinner,
    transcript::Status,
};

impl ChatView {
    fn render_transcript(&self, cx: &Context<Self>) -> AnyElement {
        if self.transcript.len() == 0 {
            return v_flex()
                .id("chat-empty")
                .test_support()
                .flex_1()
                .items_center()
                .justify_center()
                .gap_2()
                .text_color(cx.theme().muted_foreground)
                .child(div().text_2xl().text_color(spinner::accent()).child("✻"))
                .child("Ask Claude about this project.")
                .into_any_element();
        }
        let view = cx.entity().downgrade();
        MessageScroller::new(
            "chat-transcript",
            self.scroller.clone(),
            move |index, _, cx| {
                view.upgrade().map_or_else(
                    || div().into_any_element(),
                    |entity| entity.read(cx).render_row(index, &view, cx),
                )
            },
        )
        .with_row_style(StyleRefinement::default().pb(px(0.)))
        .flex_1()
        .px_4()
        .into_any_element()
    }

    /// Transcript row `index`, or the spinner row after the last entry.
    fn render_row(
        &self,
        index: usize,
        view: &gpui_kit::WeakEntity<Self>,
        cx: &gpui_kit::App,
    ) -> AnyElement {
        let on_timeline = |index: usize| {
            self.transcript
                .entry(index)
                .is_some_and(|(_, entry)| entry_view::on_timeline(entry))
        };
        let running = self.status() == Status::Running;
        let line_above = index > 0 && on_timeline(index - 1);
        let below = index + 1;
        let line_below = on_timeline(below) || (below == self.transcript.len() && running);
        let Some((id, entry)) = self.transcript.entry(index) else {
            if !running {
                return div().into_any_element();
            }
            let context = EntryContext {
                id: 0,
                toggled: false,
                view,
                root: &self.cwd,
                line_above,
                line_below: false,
            };
            return spinner::render(self.spinner_frame, self.transcript.turns(), &context, cx);
        };
        let context = EntryContext {
            id,
            toggled: self.toggled.contains(&id),
            view,
            root: &self.cwd,
            line_above: line_above && entry_view::on_timeline(entry),
            line_below: line_below && entry_view::on_timeline(entry),
        };
        entry_view::render(entry, &context, cx)
    }
}

impl Render for ChatView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let bottom = match &self.prompt {
            Some(prompt) => prompt.view.clone().into_any_element(),
            None => self.render_composer(window, cx),
        };
        v_flex()
            .id("chat")
            .test_support()
            .on_key_down(cx.listener(Self::on_chat_key))
            .size_full()
            .bg(cx.theme().background)
            .child(self.render_header(cx))
            .child(self.render_transcript(cx))
            .child(
                div()
                    .px_3()
                    .pb_3()
                    .children(self.render_limit_notice(cx))
                    .child(bottom),
            )
    }
}
