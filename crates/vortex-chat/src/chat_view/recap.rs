//! A recap for the user coming back: asked for once they have been away
//! from the window a while after a turn, so it waits for them. Claude Code
//! does the same, with the same limits.

use std::time::Duration;

use gpui_kit::{Context, Task, Window};
use vortex_agent::SessionCommand;

use super::ChatView;
use crate::transcript::Status;

/// How long the window must be inactive with the chat idle.
const AWAY: Duration = Duration::from_mins(3);
/// A shorter conversation is read faster than its recap.
const MIN_TURNS: usize = 3;

#[derive(Default)]
pub(super) struct RecapWatch {
    /// A turn ended since the last recap.
    due: bool,
    timer: Option<Task<()>>,
}

impl ChatView {
    /// A turn ended, which a recap can cover if it `ran` here.
    pub(super) fn recap_due(&mut self, ran: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.recap.due |= ran;
        self.watch_for_recap(window, cx);
    }

    /// Starts waiting when the user leaves, and stops when they come back.
    pub(super) fn watch_for_recap(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let waits = self.recap.due && !window.is_window_active();
        self.recap.timer = waits.then(|| {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(AWAY).await;
                let _ = this.update(cx, |this, _| this.ask_recap());
            })
        });
    }

    fn ask_recap(&mut self) {
        let idle = self.status() == Status::Idle && !self.is_limited();
        if idle && self.transcript.turns() >= MIN_TURNS && self.send_command(SessionCommand::Recap)
        {
            self.recap.due = false;
        }
    }
}
