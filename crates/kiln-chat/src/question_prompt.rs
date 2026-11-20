//! The agent's questions, shown in place of the composer and laid out like
//! the Claude Code extension's: a tab per question, its options with a radio
//! or checkbox, an "Other" row for the user's own words, and one button that
//! sends every answer at once.

use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, FontWeight, KeyDownEvent,
    Subscription, Task, TestSupportExt, Window,
    component::{
        ActiveTheme,
        input::{InputEvent, InputState},
        v_flex,
    },
    div,
};
use kiln_agent::{Decision, PermissionRequest};

mod layout;

use crate::{
    permission_prompt::Answered,
    question_form::{Picked, QuestionForm},
};

/// Sent back when the user dismisses the questions without answering.
const DECLINED: &str = "I'd rather not answer these questions.";

/// How long a single choice stays on screen, picked, before the next
/// question shows, so the user sees what they chose.
const SHOW_PICK: Duration = Duration::from_millis(300);

pub struct QuestionPrompt {
    request_id: String,
    form: QuestionForm,
    /// The "Other" field of each question.
    own_words: Vec<Entity<InputState>>,
    focus: FocusHandle,
    next_question: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<Answered> for QuestionPrompt {}

impl QuestionPrompt {
    pub fn new(request: &PermissionRequest, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut subscriptions = Vec::new();
        let own_words = (0..request.questions.len())
            .map(|question| {
                let input =
                    cx.new(|cx| InputState::new(window, cx).placeholder("Type your answer…"));
                subscriptions.push(cx.subscribe_in(
                    &input,
                    window,
                    move |this, _, event, window, cx| {
                        this.on_own_words(question, event, window, cx);
                    },
                ));
                input
            })
            .collect();
        Self {
            request_id: request.id.clone(),
            form: QuestionForm::new(request.questions.clone()),
            own_words,
            focus: cx.focus_handle(),
            next_question: None,
            _subscriptions: subscriptions,
        }
    }

    fn on_own_words(
        &mut self,
        question: usize,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::Change => {
                if let Some(input) = self.own_words.get(question) {
                    let text = input.read(cx).value();
                    self.form.set_other_text(question, &text);
                }
            }
            InputEvent::PressEnter { .. } => {
                self.form.next();
                window.focus(&self.focus, cx);
            }
            _ => return,
        }
        cx.notify();
    }

    fn pick(&mut self, row: usize, window: &mut Window, cx: &mut Context<Self>) {
        let question = self.form.current();
        match self.form.pick(row) {
            Some(Picked::Single) => self.show_next_soon(question, window, cx),
            Some(Picked::Other) if self.form.is_picked(question, row) => {
                if let Some(input) = self.own_words.get(question) {
                    input.update(cx, |input, cx| input.focus(window, cx));
                }
            }
            _ => {}
        }
        cx.notify();
    }

    fn show_next_soon(&mut self, question: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.next_question = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(SHOW_PICK).await;
            let _ = this.update(cx, |this, cx| this.move_on_from(question, cx));
        }));
    }

    /// Shows the next question unless the user went elsewhere meanwhile.
    fn move_on_from(&mut self, question: usize, cx: &mut Context<Self>) {
        if self.form.current() == question {
            self.form.next();
            cx.notify();
        }
    }

    fn show(&mut self, question: usize, cx: &mut Context<Self>) {
        self.next_question = None;
        self.form.show(question);
        cx.notify();
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        if self.form.is_complete() {
            self.answer(Decision::Answer(self.form.answers()), cx);
        }
    }

    fn decline(&mut self, cx: &mut Context<Self>) {
        self.answer(
            Decision::Deny {
                feedback: Some(DECLINED.to_owned()),
            },
            cx,
        );
    }

    fn answer(&self, decision: Decision, cx: &mut Context<Self>) {
        cx.emit(Answered {
            request_id: self.request_id.clone(),
            decision,
        });
    }

    fn on_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        if !self.focus.is_focused(window) {
            if key == "escape" {
                window.focus(&self.focus, cx);
                cx.stop_propagation();
            }
            return;
        }
        match key {
            "up" => self.form.move_highlight(-1),
            "down" => self.form.move_highlight(1),
            "left" => self.form.previous(),
            "right" => {
                self.form.next();
            }
            "enter" | "space" => self.pick(self.form.highlighted(), window, cx),
            "1" => self.submit(cx),
            "escape" => self.decline(cx),
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }
}

impl Focusable for QuestionPrompt {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for QuestionPrompt {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let question = self.form.current_question().cloned();
        let rows = question
            .as_ref()
            .map_or_else(Vec::new, |question| self.render_rows(question, window, cx));
        let tabs = self.render_tabs(cx);
        let submit = self.render_submit(cx);
        let theme = cx.theme();
        v_flex()
            .id("question-prompt")
            .test_support()
            .aria_label("Claude's questions")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .gap_2()
            .p_3()
            .rounded(theme.radius * 2.)
            .border_1()
            .border_color(theme.primary)
            .bg(theme.secondary)
            .child(tabs)
            .children(question.map(|question| {
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .child(question.text)
            }))
            .child(v_flex().gap_0p5().children(rows))
            .child(submit)
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("Esc to cancel"),
            )
    }
}
