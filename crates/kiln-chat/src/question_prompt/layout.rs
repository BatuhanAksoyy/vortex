//! How the question prompt draws its parts: the tabs, the option rows with
//! their radio or checkbox, and the submit button.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Context, FontWeight, SharedString, TestSupportExt, Window,
    component::{
        ActiveTheme, Icon, IconName, Sizable,
        button::{Button, ButtonVariants},
        h_flex,
        input::Input,
        v_flex,
    },
    div, px, transparent_black,
};
use kiln_agent::Question;

use super::QuestionPrompt;
use crate::{
    scale::{ScaledButton as _, ScaledIcon as _, ScaledText as _},
    spinner,
};

impl QuestionPrompt {
    pub(super) fn render_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let tabs: Vec<AnyElement> = self
            .form
            .questions()
            .iter()
            .enumerate()
            .map(|(index, question)| {
                let active = index == self.form.current();
                let muted = !active && self.form.is_answered(index);
                div()
                    .id(SharedString::from(format!("question-tab-{index}")))
                    .test_support()
                    .px_0p5()
                    .py_1()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .border_b_2()
                    .border_color(if active {
                        spinner::accent(cx)
                    } else {
                        transparent_black()
                    })
                    .text_color(if muted {
                        theme.muted_foreground
                    } else {
                        theme.foreground
                    })
                    .cursor_pointer()
                    .child(tab_label(question, index))
                    .on_click(cx.listener(move |this, _, _, cx| this.show(index, cx)))
                    .into_any_element()
            })
            .collect();
        h_flex()
            .gap_3()
            .items_center()
            .children(tabs)
            .child(div().flex_1())
            .child(
                Button::new("question-close")
                    .ghost()
                    .row_icon(IconName::Close)
                    .tooltip("Decline to answer")
                    .on_click(cx.listener(|this, _, _, cx| this.decline(cx))),
            )
    }

    pub(super) fn render_rows(
        &self,
        question: &Question,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let index = self.form.current();
        let mut rows: Vec<AnyElement> = question
            .options
            .iter()
            .enumerate()
            .map(|(row, option)| {
                self.option_row(row, question.multi_select, window, cx)
                    .child(option_text(&option.label, &option.description, cx))
                    .into_any_element()
            })
            .collect();
        let other = self.form.other_row();
        let own_words = self
            .own_words
            .get(index)
            .filter(|_| self.form.is_picked(index, other))
            .map(|input| div().mt_1p5().child(Input::new(input).small()));
        rows.push(
            self.option_row(other, question.multi_select, window, cx)
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(option_text("Other", "", cx))
                        .children(own_words),
                )
                .into_any_element(),
        );
        rows
    }

    fn option_row(
        &self,
        row: usize,
        multi: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl ParentElement + IntoElement + use<> {
        let theme = cx.theme();
        let picked = self.form.is_picked(self.form.current(), row);
        let highlighted = row == self.form.highlighted() && self.focus.is_focused(window);
        h_flex()
            .id(SharedString::from(format!("question-option-{row}")))
            .test_support()
            .items_start()
            .gap_2p5()
            .px_1()
            .py_1()
            .rounded(theme.radius)
            .border_l_2()
            .border_color(if highlighted {
                spinner::accent(cx)
            } else {
                transparent_black()
            })
            .cursor_pointer()
            .hover(|this| this.bg(theme.accent))
            .child(indicator(multi, picked, cx))
            .on_click(cx.listener(move |this, _, window, cx| this.pick(row, window, cx)))
    }

    pub(super) fn render_submit(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let ready = self.form.is_complete();
        h_flex()
            .id("question-submit")
            .test_support()
            .gap_1()
            .px_3()
            .py_1p5()
            .rounded(theme.radius)
            .bg(spinner::accent(cx))
            .text_color(theme.primary_foreground)
            .font_weight(FontWeight::MEDIUM)
            .when(!ready, |this| this.opacity(0.5))
            .when(ready, |this| {
                this.cursor_pointer()
                    .on_click(cx.listener(|this, _, _, cx| this.submit(cx)))
            })
            .child(div().opacity(0.6).child("1"))
            .child("Submit answers")
    }
}

/// The tab's name: the question's short header, or its number.
fn tab_label(question: &Question, index: usize) -> SharedString {
    if question.header.is_empty() {
        format!("Question {}", index + 1).into()
    } else {
        question.header.clone().into()
    }
}

fn option_text(label: &str, description: &str, cx: &App) -> impl IntoElement + use<> {
    v_flex()
        .flex_1()
        .min_w_0()
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .child(label.to_owned()),
        )
        .when(!description.is_empty(), |this| {
            this.child(
                div()
                    .text_caption()
                    .text_color(cx.theme().muted_foreground)
                    .child(description.to_owned()),
            )
        })
}

/// A radio for a single choice, a checkbox for multiple.
fn indicator(multi: bool, picked: bool, cx: &App) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let mark = div()
        .flex_none()
        .mt_0p5()
        .size_4()
        .flex()
        .items_center()
        .justify_center()
        .border_1()
        .border_color(if picked {
            spinner::accent(cx)
        } else {
            theme.muted_foreground.opacity(0.6)
        });
    if multi {
        mark.rounded(px(3.)).when(picked, |this| {
            this.bg(spinner::accent(cx)).child(
                Icon::new(IconName::Check)
                    .in_row()
                    .text_color(theme.primary_foreground),
            )
        })
    } else {
        mark.rounded_full().when(picked, |this| {
            this.child(div().size_2().rounded_full().bg(spinner::accent(cx)))
        })
    }
}
