//! Keys for the message being written and the turn under way, as in the
//! CLI: Up and Down go through the messages sent, and Escape stops the turn,
//! putting the message back to edit when nothing came in reply yet.

use gpui_kit::{Context, Focusable as _, KeyDownEvent, Window};

use super::ChatView;
use crate::transcript::Status;

impl ChatView {
    /// Up on the first line: the message sent before the one shown, while
    /// the box is empty or still holds a recalled message unchanged.
    pub(super) fn recall_older(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let (text, cursor) = self.composer_text(cx);
        if text[..cursor].contains('\n') {
            return false;
        }
        let older = match &self.recalled {
            Some((index, shown)) if *shown == text => index + 1,
            _ if text.is_empty() => 0,
            _ => return false,
        };
        let Some(message) = self.transcript.sent().nth(older).map(str::to_owned) else {
            return false;
        };
        self.show_in_composer(&message, window, cx);
        self.recalled = Some((older, message));
        true
    }

    /// Down on the last line of a recalled message: the one sent after it,
    /// or an empty box past the newest.
    pub(super) fn recall_newer(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let (text, cursor) = self.composer_text(cx);
        let Some((index, shown)) = self.recalled.take() else {
            return false;
        };
        if shown != text || text[cursor..].contains('\n') {
            self.recalled = (shown == text).then_some((index, shown));
            return false;
        }
        let newer = index
            .checked_sub(1)
            .and_then(|newer| Some((newer, self.transcript.sent().nth(newer)?.to_owned())));
        let message = newer.as_ref().map_or("", |(_, message)| message.as_str());
        self.show_in_composer(message, window, cx);
        self.recalled = newer;
        true
    }

    /// Escape while a turn runs interrupts it. A message nothing came back
    /// for yet leaves the transcript and returns to the empty box, so it
    /// can be edited and sent again. `false` when no turn runs.
    pub(super) fn stop_turn(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.status() != Status::Running {
            return false;
        }
        self.interrupt(window, cx);
        if !self.composer_text(cx).0.is_empty() {
            return true;
        }
        if let Some((row, message)) = self.transcript.withdraw() {
            self.scroller
                .update(cx, |scroller, cx| scroller.splice(row..row + 1, 0, cx));
            self.show_in_composer(&message.text, window, cx);
            self.restore_attachments(message, cx);
            cx.notify();
        }
        true
    }

    /// Escape with the keyboard outside the box, e.g. after selecting text
    /// in the transcript; the box takes it as an action of its own. With a
    /// panel open, Escape is the panel's: it closes it.
    pub(super) fn on_chat_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let keystroke = &event.keystroke;
        if keystroke.key == "f" && keystroke.modifiers.secondary(cx) {
            self.open_find(window, cx);
            cx.stop_propagation();
            return;
        }
        let in_box = self.composer.read(cx).focus_handle(cx).is_focused(window);
        let panel_open = self.open_panel.is_some();
        if event.keystroke.key == "escape" && !in_box && !panel_open && self.stop_turn(window, cx) {
            cx.stop_propagation();
        }
    }

    fn composer_text(&self, cx: &Context<Self>) -> (String, usize) {
        let composer = self.composer.read(cx);
        let text = composer.value().to_string();
        let cursor = composer.cursor().min(text.len());
        (text, cursor)
    }

    fn show_in_composer(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let end = text.len();
        self.composer.update(cx, |composer, cx| {
            composer.replace_all(text.to_owned(), window, cx);
            composer.set_selected_range(end..end, cx);
        });
    }
}
