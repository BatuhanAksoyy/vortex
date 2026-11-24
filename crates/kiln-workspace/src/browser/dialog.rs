//! A question the page asks, over the page until it is answered.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, Context, Entity, FocusHandle, Focusable, FontWeight, KeyDownEvent, Subscription,
    TestSupportExt, Window,
    component::{
        ActiveTheme, Sizable as _,
        button::{Button, ButtonVariants as _},
        h_flex,
        input::{Input, InputEvent, InputState},
        v_flex,
    },
    div, px,
};
use kiln_browser::{Dialog, DialogKind};

use super::Browser;

const WIDTH: f32 = 360.;
/// A page may ask with any amount of text.
const MAX_MESSAGE: f32 = 240.;

pub(super) struct OpenDialog {
    dialog: Dialog,
    /// A prompt's answer.
    answer: Entity<InputState>,
    focus: FocusHandle,
    _enter: Subscription,
}

impl Browser {
    pub(super) fn ask(&mut self, dialog: Dialog, window: &mut Window, cx: &mut Context<Self>) {
        let suggested = dialog.suggested.clone();
        let answer = cx.new(|cx| InputState::new(window, cx).default_value(suggested));
        let enter = cx.subscribe_in(&answer, window, |this, _, event, window, cx| {
            if let InputEvent::PressEnter { .. } = event {
                this.answer(true, window, cx);
            }
        });
        let open = OpenDialog {
            dialog,
            answer,
            focus: cx.focus_handle(),
            _enter: enter,
        };
        // Only where the user is: a page must not take the keyboard from
        // the chat.
        if self.focus.contains_focused(window, cx) {
            open.focus_answer(window, cx);
        }
        self.menu = None;
        self.dialog = Some(open);
        cx.notify();
    }

    /// The page was answered, or went away.
    pub(super) fn dialog_closed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.dialog.take() else {
            return;
        };
        let answer = open.answer.focus_handle(cx);
        if open.focus.contains_focused(window, cx) || answer.is_focused(window) {
            self.focus.focus(window, cx);
        }
        cx.notify();
    }

    fn answer(&mut self, accept: bool, window: &mut Window, cx: &mut Context<Self>) {
        if let (Some(open), Some(page)) = (&self.dialog, self.page()) {
            let text = open.answer.read(cx).value();
            page.answer_dialog(accept, &text);
        }
        self.dialog_closed(window, cx);
    }

    fn on_dialog_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = &self.dialog else {
            return;
        };
        // Enter on a button is the button's.
        let card = open.focus.is_focused(window);
        match event.keystroke.key.as_str() {
            "enter" if card => self.answer(true, window, cx),
            "escape" => self.answer(false, window, cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    pub(super) fn render_dialog(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let open = self.dialog.as_ref()?;
        let theme = cx.theme();
        let card = v_flex()
            .id("browser-dialog")
            .test_support()
            .track_focus(&open.focus)
            .on_key_down(cx.listener(Self::on_dialog_key))
            .w(px(WIDTH))
            .max_w_full()
            .gap_3()
            .p_4()
            .rounded(theme.radius_lg)
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .text_color(theme.popover_foreground)
            .shadow_lg()
            .child(div().font_weight(FontWeight::SEMIBOLD).child(open.title()))
            .child(
                div()
                    .id("browser-dialog-message")
                    .max_h(px(MAX_MESSAGE))
                    .overflow_y_scroll()
                    .text_sm()
                    .child(open.message()),
            )
            .when(open.dialog.kind == DialogKind::Prompt, |card| {
                card.child(Input::new(&open.answer).small())
            })
            .child(open.buttons(cx));
        let over = div()
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .justify_center()
            .items_start()
            .p_4()
            .bg(theme.overlay)
            .child(card);
        Some(over.into_any_element())
    }
}

impl OpenDialog {
    fn focus_answer(&self, window: &mut Window, cx: &mut Context<Browser>) {
        match self.dialog.kind {
            DialogKind::Prompt => self.answer.focus_handle(cx).focus(window, cx),
            DialogKind::Alert | DialogKind::Confirm | DialogKind::Leave => {
                self.focus.focus(window, cx);
            }
        }
    }

    fn title(&self) -> String {
        if self.dialog.kind == DialogKind::Leave {
            return "Leave this page?".to_owned();
        }
        let url = &self.dialog.url;
        let rest = url.split_once("://").map_or(url.as_str(), |(_, rest)| rest);
        let host = rest.split('/').next().filter(|host| !host.is_empty());
        format!("{} says", host.unwrap_or("This page"))
    }

    fn message(&self) -> String {
        match self.dialog.kind {
            DialogKind::Leave => "Changes you made may not be saved.".to_owned(),
            DialogKind::Alert | DialogKind::Confirm | DialogKind::Prompt => {
                self.dialog.message.clone()
            }
        }
    }

    fn buttons(&self, cx: &Context<Browser>) -> impl IntoElement + use<> {
        let (yes, no) = match self.dialog.kind {
            DialogKind::Alert => ("OK", None),
            DialogKind::Confirm | DialogKind::Prompt => ("OK", Some("Cancel")),
            DialogKind::Leave => ("Leave", Some("Stay")),
        };
        let refuse = no.map(|label| {
            Button::new("browser-dialog-no")
                .small()
                .label(label)
                .on_click(cx.listener(|this, _, window, cx| this.answer(false, window, cx)))
        });
        let accept = Button::new("browser-dialog-yes")
            .primary()
            .small()
            .label(yes)
            .on_click(cx.listener(|this, _, window, cx| this.answer(true, window, cx)));
        h_flex()
            .justify_end()
            .gap_2()
            .children(refuse)
            .child(accept)
    }
}
