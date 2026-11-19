//! The chat panel's layout, after the Claude Code VS Code panel: a title
//! bar, the transcript timeline, and the composer (or the permission prompt
//! in its place).

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, Context, Focusable, StyleRefinement, TestSupportExt, Window,
    component::{
        ActiveTheme, Icon, IconName, Sizable, h_flex, input::Textarea,
        message_scroller::MessageScroller, v_flex,
    },
    div, px, white,
};

use super::{ChatView, Mode};
use crate::{
    entry_view::{self, EntryContext},
    spinner,
    transcript::Status,
};

const SEND_BUTTON_SIZE: f32 = 26.;

impl ChatView {
    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let title = self
            .transcript
            .title()
            .unwrap_or("New conversation")
            .to_owned();
        h_flex()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(theme.border)
            .child(div().flex_1().min_w_0().truncate().child(title))
            .child(
                div()
                    .id("chat-new")
                    .test_support()
                    .aria_label("New conversation")
                    .p_1()
                    .rounded(theme.radius)
                    .cursor_pointer()
                    .hover(|style| style.bg(gpui_kit::black().opacity(0.08)))
                    .child(Icon::new(IconName::Plus).small())
                    .on_click(cx.listener(|this, _, window, cx| this.new_conversation(window, cx))),
            )
    }

    fn render_transcript(&self, cx: &Context<Self>) -> AnyElement {
        if self.transcript.len() == 0 {
            return v_flex()
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

    fn render_composer(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let running = self.status() == Status::Running;
        let focused = self.composer.read(cx).focus_handle(cx).is_focused(window);
        let mode = match self.mode {
            Mode::AskBeforeEdits => "Ask before edits",
            Mode::EditAutomatically => "Edit automatically",
        };
        let action = if running {
            action_button(
                "chat-stop",
                IconName::Square,
                cx.theme().muted_foreground,
                cx,
            )
            .on_click(cx.listener(|this, _, window, cx| this.interrupt(window, cx)))
        } else {
            action_button("chat-send", IconName::ArrowUp, spinner::accent(), cx)
                .on_click(cx.listener(|this, _, window, cx| this.send(window, cx)))
        };
        let theme = cx.theme();
        v_flex()
            .rounded(theme.radius * 2.)
            .border_1()
            .border_color(if focused {
                spinner::accent()
            } else {
                theme.border
            })
            .bg(theme.background)
            .child(
                div()
                    .id("chat-composer")
                    .test_support()
                    .px_1()
                    .pt_1()
                    .child(Textarea::new(&self.composer).appearance(false)),
            )
            .child(
                h_flex()
                    .gap_3()
                    .pl_3()
                    .pr_1p5()
                    .py_1p5()
                    .border_t_1()
                    .border_color(theme.border)
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(mode_label(mode))
                    .when_some(self.transcript.model(), |this, model| {
                        this.child(model.to_owned())
                    })
                    .child(div().flex_1())
                    .child(action),
            )
            .into_any_element()
    }
}

fn action_button(
    id: &'static str,
    icon: IconName,
    background: gpui_kit::Hsla,
    cx: &Context<ChatView>,
) -> impl StatefulInteractiveElement + IntoElement {
    div()
        .id(id)
        .test_support()
        .flex()
        .items_center()
        .justify_center()
        .size(px(SEND_BUTTON_SIZE))
        .rounded(cx.theme().radius)
        .bg(background)
        .cursor_pointer()
        .child(Icon::new(icon).small().text_color(white()))
}

fn mode_label(mode: &'static str) -> impl IntoElement {
    h_flex()
        .id("chat-mode")
        .test_support()
        .gap_1()
        .child(Icon::new(gpui_kit::assets::IconName::Pencil).xsmall())
        .child(mode)
}

impl Render for ChatView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let bottom = match &self.prompt {
            Some((prompt, _)) => prompt.clone().into_any_element(),
            None => self.render_composer(window, cx),
        };
        v_flex()
            .id("chat")
            .test_support()
            .size_full()
            .bg(cx.theme().background)
            .child(self.render_header(cx))
            .child(self.render_transcript(cx))
            .child(div().px_3().pb_3().child(bottom))
    }
}
