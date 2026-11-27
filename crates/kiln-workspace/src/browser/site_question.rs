//! Asking the user before the agent uses a site off this machine.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, Context, FontWeight, TestSupportExt,
    component::{
        ActiveTheme,
        button::{Button, ButtonVariants as _},
        h_flex, v_flex,
    },
    div, px,
};
use kiln_chat::scale::ScaledButton as _;

use super::Browser;

const WIDTH: f32 = 360.;

impl Browser {
    pub(super) fn render_site_question(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let site = &self.agent.waiting.first()?.site;
        let who = self.agent.who();
        let theme = cx.theme();
        let buttons = h_flex()
            .justify_end()
            .gap_2()
            .child(
                Button::new("browser-site-deny")
                    .control()
                    .label("Don't Allow")
                    .on_click(cx.listener(|this, _, window, cx| this.decide(false, window, cx))),
            )
            .child(
                Button::new("browser-site-allow")
                    .primary()
                    .control()
                    .label("Allow")
                    .on_click(cx.listener(|this, _, window, cx| this.decide(true, window, cx))),
            );
        let card = v_flex()
            .id("browser-site-question")
            .test_support()
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
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(format!("Let {who} use {site}?")),
            )
            .child(div().text_sm().child(format!(
                "{who} wants to open and use pages on {site}. It would see \
                 what you are signed in to there, until this chat is closed."
            )))
            .child(buttons);
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
