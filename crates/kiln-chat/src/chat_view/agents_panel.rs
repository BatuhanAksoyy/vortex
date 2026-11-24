//! The subagents button in the composer strip. It counts the subagents
//! running and opens, over this chat alone, the view that lists them all.

use gpui_kit::prelude::*;
use gpui_kit::{
    Context, Focusable as _, Window,
    component::{
        ActiveTheme, Sizable,
        button::{Button, ButtonVariants},
    },
    div,
};

use super::{ChatView, agents_dialog::AgentsDialog};
use crate::transcript::EntryId;

impl ChatView {
    /// `None` until the conversation starts a subagent.
    pub(super) fn agents_button(&self, cx: &Context<Self>) -> Option<impl IntoElement + use<>> {
        let total = self.transcript.subagents().count();
        if total == 0 {
            return None;
        }
        let running = self
            .transcript
            .subagents()
            .filter(|card| card.agent.end.is_none())
            .count();
        Some(
            Button::new("chat-agents")
                .ghost()
                .xsmall()
                .label(button_label(running, total))
                .text_color(if running > 0 {
                    crate::spinner::accent()
                } else {
                    cx.theme().muted_foreground
                })
                .tooltip("Subagents")
                .on_click(cx.listener(|this, _, window, cx| this.open_agents(window, cx))),
        )
    }

    fn open_agents(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let chat = cx.entity();
        let dialog = cx.new(|cx| AgentsDialog::new(&chat, cx));
        let focus = dialog.read(cx).focus_handle();
        window.focus(&focus, cx);
        self.agents = Some(dialog);
        cx.notify();
    }

    pub(super) fn close_agents(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.agents.take().is_some() {
            let composer = self.composer.read(cx).focus_handle(cx);
            window.focus(&composer, cx);
            cx.notify();
        }
    }

    /// The subagents view over the transcript and composer of this chat;
    /// the backdrop keeps clicks from reaching the chat under it, while
    /// other chats stay usable.
    pub(super) fn render_agents(&self, cx: &Context<Self>) -> Option<impl IntoElement + use<>> {
        let dialog = self.agents.clone()?;
        Some(
            div()
                .id("chat-agents-overlay")
                .absolute()
                .inset_0()
                .p_3()
                .occlude()
                .bg(cx.theme().overlay)
                .child(dialog),
        )
    }

    /// Opens the card of entry `id` and scrolls to it.
    pub(super) fn reveal_subagent(&mut self, id: EntryId, cx: &mut Context<Self>) {
        let Some(index) = self
            .transcript
            .subagents()
            .find_map(|card| (card.id == id).then_some(card.index))
        else {
            return;
        };
        self.toggled.insert(id);
        self.scroller.update(cx, |scroller, cx| {
            scroller.remeasure(cx);
            scroller.scroll_to_item(index, cx);
        });
        cx.notify();
    }
}

/// `3 agents running`, or `5 agents` once all are done.
fn button_label(running: usize, total: usize) -> String {
    let agents = if total == 1 { "agent" } else { "agents" };
    if running == 0 {
        return format!("{total} {agents}");
    }
    let agents = if running == 1 { "agent" } else { "agents" };
    format!("{running} {agents} running")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_button_counts_running_agents_until_all_are_done() {
        assert_eq!(button_label(1, 1), "1 agent running");
        assert_eq!(button_label(3, 10), "3 agents running");
        assert_eq!(button_label(0, 1), "1 agent");
        assert_eq!(button_label(0, 10), "10 agents");
    }
}
