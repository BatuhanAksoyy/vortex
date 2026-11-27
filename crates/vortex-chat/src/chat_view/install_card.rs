//! What a chat shows in place of its transcript while its harness's CLI is
//! missing: which one, and a button that installs it.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Context, TestSupportExt as _, Window,
    component::{
        ActiveTheme as _, Disableable as _, Sizable as _,
        button::{Button, ButtonVariants as _},
        spinner::Spinner,
        v_flex,
    },
    div,
};

use super::{ChatEvent, ChatView};
use crate::{
    agent_setup::{AgentSetup, Setup},
    backends::title,
};

impl ChatView {
    /// Marks the chat's CLI missing, as starting it found.
    pub(super) fn cli_missing(&mut self, cx: &mut Context<Self>) {
        let agent = self.backend.name();
        // A running installation, or why the last one failed, stays shown.
        if !AgentSetup::is_missing(agent, cx) {
            AgentSetup::set(agent, Some(Setup::Missing), cx);
        }
        cx.notify();
    }

    /// The CLI of harness `agent` was installed or found missing: the chat
    /// redraws, as its picker offers only installed harnesses, and a chat on
    /// it starts its process once it is there.
    pub fn cli_checked(&mut self, agent: &str, window: &mut Window, cx: &mut Context<Self>) {
        let own = agent == self.backend.name();
        if own && !AgentSetup::is_missing(agent, cx) && self.session.is_none() {
            self.ensure_session(window, cx);
        }
        cx.notify();
    }

    pub(super) fn render_install_card(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let agent = self.backend.name();
        let setup = AgentSetup::of(agent, cx)?;
        let name = title(agent);
        let installing = setup == Setup::Installing;
        let button = Button::new("chat-install-agent")
            .primary()
            .small()
            .label(if installing {
                format!("Installing {name}…")
            } else {
                format!("Install {name}")
            })
            .disabled(installing)
            .on_click(cx.listener(move |_, _, _, cx| cx.emit(ChatEvent::InstallAgent(agent))));
        let card = v_flex()
            .id("chat-install")
            .test_support()
            .flex_1()
            .items_center()
            .justify_center()
            .gap_3()
            .px_6()
            .child(
                div()
                    .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                    .child(format!("{name} is not installed")),
            )
            .child(note(
                format!("Vortex runs {name} for this chat. Install it, then send your message."),
                cx,
            ))
            .child(button)
            .when(installing, |card| card.child(Spinner::new().small()))
            .children(match setup {
                Setup::Failed(why) => Some(note(why, cx)),
                _ => None,
            })
            .children(self.render_agent_picker(cx))
            .into_any_element();
        Some(card)
    }
}

fn note(text: String, cx: &App) -> impl IntoElement {
    div()
        .max_w_96()
        .text_center()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(text)
}
