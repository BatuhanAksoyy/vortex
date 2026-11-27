//! The composer's text box: what typing in it and sending from it does,
//! and the message the agent predicts, shown in it while it is empty and
//! taken with Tab.

use gpui_kit::prelude::*;
use gpui_kit::{
    Context, Entity, Subscription, Window,
    component::input::{InputEvent, TextareaState},
};

use vortex_agent::AgentEvent;

use super::ChatView;

const MAX_ROWS: usize = 10;
const PLACEHOLDER: &str = "Ask Claude to edit, explain or run something…";

pub(super) fn new_composer(
    window: &mut Window,
    cx: &mut Context<ChatView>,
) -> Entity<TextareaState> {
    cx.new(|cx| {
        let mut state = TextareaState::new(window, cx)
            .auto_grow(1, MAX_ROWS)
            .placeholder(PLACEHOLDER);
        state.set_submit_on_enter(true, cx);
        state
    })
}

impl ChatView {
    pub(super) fn follow_composer(
        composer: &Entity<TextareaState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Subscription {
        cx.subscribe_in(composer, window, |this, _, event, window, cx| match event {
            InputEvent::PressEnter { shift: false, .. } => this.send(window, cx),
            InputEvent::Change => {
                // Typing a message puts the picker away; the text box
                // emptying as a command runs leaves it open.
                if !this.composer.read(cx).value().is_empty() {
                    this.close_model_picker(cx);
                }
                this.sync_slash_menu(cx);
                if !this.composer.read(cx).value().is_empty() {
                    this.drop_suggestion(window, cx);
                }
            }
            _ => {}
        })
    }

    pub(super) fn update_suggestion(
        &mut self,
        event: &AgentEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            AgentEvent::PromptSuggestion(suggestion) => {
                self.suggest(suggestion.clone(), window, cx);
            }
            // A model request starts a turn, also one the user did not.
            AgentEvent::ConversationReset
            | AgentEvent::Exited { .. }
            | AgentEvent::PromptTokens(_) => {
                self.drop_suggestion(window, cx);
            }
            _ => {}
        }
    }

    /// Offers `suggestion` in the composer, unless something is typed.
    fn suggest(&mut self, suggestion: String, window: &mut Window, cx: &mut Context<Self>) {
        if !self.composer.read(cx).value().is_empty() {
            return;
        }
        let shown = format!("{suggestion}  (Tab to use)");
        self.composer.update(cx, |composer, cx| {
            composer.set_placeholder(shown, window, cx);
        });
        self.suggestion = Some(suggestion);
    }

    pub(super) fn drop_suggestion(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.suggestion.take().is_some() {
            self.composer.update(cx, |composer, cx| {
                composer.set_placeholder(PLACEHOLDER, window, cx);
            });
        }
    }

    /// Puts the suggestion in the composer to send or change; `false` when
    /// none is offered.
    pub(super) fn take_suggestion(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(suggestion) = self.suggestion.clone() else {
            return false;
        };
        self.drop_suggestion(window, cx);
        self.show_in_composer(&suggestion, window, cx);
        true
    }
}
