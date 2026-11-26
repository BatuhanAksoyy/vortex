//! The subagents button in the composer strip. It counts the subagents
//! running, with a dot lit while any does, and opens, over this chat alone,
//! the view that lists them all.

use gpui_kit::prelude::*;
use gpui_kit::{
    Context, Focusable as _, Window,
    component::{
        ActiveTheme, Disableable as _,
        button::{Button, ButtonVariants},
        h_flex,
    },
    div,
};

use kiln_agent::SessionCommand;

use super::{ChatView, agents_dialog::AgentsDialog};
use crate::{
    scale::ScaledButton as _,
    transcript::{EntryId, Status},
};

impl ChatView {
    /// Shown from the start, so the strip does not shift when the first
    /// subagent starts; the count of running ones keeps its width steady.
    pub(super) fn agents_button(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let agents = self.transcript.subagents();
        let (total, running) = (agents.len(), agents.running());
        let theme = cx.theme();
        let dot = if running > 0 {
            theme.success
        } else {
            theme.muted_foreground.opacity(0.5)
        };
        let label = format!("Agents {running}");
        Button::new("chat-agents")
            .ghost()
            .control()
            .px_1p5()
            .accessibility_label(label.clone())
            .child(
                h_flex()
                    .gap_1p5()
                    .child(div().flex_none().size_2().rounded_full().bg(dot))
                    .child(label),
            )
            .text_color(theme.muted_foreground)
            .tooltip(tooltip(running, total))
            .disabled(total == 0)
            .on_click(cx.listener(|this, _, window, cx| this.open_agents(None, window, cx)))
    }

    /// Opens the view on subagent `shown`, or the first in its list.
    pub fn open_agents(
        &mut self,
        shown: Option<EntryId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let chat = cx.entity();
        let dialog = cx.new(|cx| AgentsDialog::new(&chat, shown, cx));
        let focus = dialog.read(cx).focus_handle();
        window.focus(&focus, cx);
        self.agents = Some(dialog);
        cx.notify();
    }

    pub fn stop_subagent(&mut self, id: EntryId, cx: &mut Context<Self>) {
        let Some(agent) = self.transcript.subagents().get(id) else {
            return;
        };
        let stop = SessionCommand::StopSubagent(agent.call.id.clone());
        if self.send_command(stop) {
            self.transcript.stop_asked(id);
        }
        cx.notify();
    }

    /// Whether the button stops the session: while a turn runs, and while
    /// only subagents in the background do, until a message is written.
    pub(super) fn can_stop(&self, cx: &Context<Self>) -> bool {
        let empty = self.attachments.is_empty() && self.composer.read(cx).value().is_empty();
        match self.status() {
            Status::Running => true,
            Status::Idle => empty && self.transcript.subagents().running() > 0,
            Status::Ended => false,
        }
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

    /// Opens the card of subagent `id`, and those around it, and scrolls to
    /// their row.
    pub(super) fn reveal_subagent(&mut self, id: EntryId, cx: &mut Context<Self>) {
        let path = self.transcript.subagents().path(id);
        let Some(index) = path.first().map(|agent| agent.row) else {
            return;
        };
        self.toggled.extend(path.iter().map(|agent| agent.id));
        self.scroller.update(cx, |scroller, cx| {
            scroller.remeasure(cx);
            scroller.scroll_to_item(index, cx);
        });
        cx.notify();
    }
}

/// `3 agents running`, or `5 agents` once all are done.
/// How many subagents run, out of all the conversation started.
fn tooltip(running: usize, total: usize) -> String {
    match total {
        0 => "No subagents yet".to_owned(),
        1 => format!("Subagents: {running} running of 1"),
        total => format!("Subagents: {running} running of {total}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tooltip_says_how_many_run_of_all() {
        assert_eq!(tooltip(0, 0), "No subagents yet");
        assert_eq!(tooltip(1, 1), "Subagents: 1 running of 1");
        assert_eq!(tooltip(0, 10), "Subagents: 0 running of 10");
    }
}
