//! A dialog that asks for a file's or folder's name.

use std::rc::Rc;

use gpui_kit::prelude::*;
use gpui_kit::{
    App, Context, Entity, SharedString, Subscription, TestSupportExt, Window,
    component::{
        ActiveTheme as _, WindowExt as _,
        button::{Button, ButtonVariants as _},
        h_flex,
        input::{Input, InputEvent, InputState},
        v_flex,
    },
    div,
};

/// Does what the name is for; an error keeps the dialog open and is shown.
type Submit = Rc<dyn Fn(&str, &mut Window, &mut App) -> anyhow::Result<()>>;

struct NamePrompt {
    input: Entity<InputState>,
    action: SharedString,
    submit: Submit,
    error: Option<String>,
    _entered: Subscription,
}

/// What a name is asked for.
pub struct Question {
    pub title: SharedString,
    /// The button that goes ahead.
    pub action: &'static str,
    pub initial: String,
}

pub fn ask(
    question: Question,
    submit: impl Fn(&str, &mut Window, &mut App) -> anyhow::Result<()> + 'static,
    window: &mut Window,
    cx: &mut App,
) {
    let title = question.title.clone();
    let prompt = cx.new(|cx| NamePrompt::new(question, Rc::new(submit), window, cx));
    let input = prompt.read(cx).input.clone();
    window.open_dialog(cx, move |dialog, _, _| {
        dialog
            .title(title.clone())
            .child(prompt.clone())
            // Enter also confirms the dialog, which would close it over an
            // error; the prompt closes it itself.
            .on_ok(|_, _, _| false)
    });
    input.update(cx, |input, cx| input.focus(window, cx));
}

impl NamePrompt {
    fn new(
        question: Question,
        submit: Submit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Name")
                .default_value(question.initial)
        });
        let entered = cx.subscribe_in(&input, window, |this, _, event, window, cx| match event {
            InputEvent::PressEnter { .. } => this.submit(window, cx),
            InputEvent::Change => {
                this.error = None;
                cx.notify();
            }
            _ => {}
        });
        Self {
            input,
            action: question.action.into(),
            submit,
            error: None,
            _entered: entered,
        }
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.input.read(cx).value();
        let submit = self.submit.clone();
        match submit(name.trim(), window, cx) {
            Ok(()) => window.close_dialog(cx),
            Err(error) => {
                self.error = Some(format!("{error:#}"));
                cx.notify();
            }
        }
    }
}

impl Render for NamePrompt {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_3()
            .child(Input::new(&self.input).aria_label("Name"))
            .when_some(self.error.clone(), |this, error| {
                this.child(div().text_sm().text_color(cx.theme().danger).child(error))
            })
            .child(
                h_flex()
                    .id("name-prompt-actions")
                    .test_support()
                    .gap_2()
                    .justify_end()
                    .child(
                        Button::new("name-prompt-cancel")
                            .label("Cancel")
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new("name-prompt-submit")
                            .primary()
                            .label(self.action.clone())
                            .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
                    ),
            )
    }
}
