//! Keys the composer's menus take from the text box while one is open: the
//! model picker first, then the `/` menu. With neither open, Up, Down and
//! Escape go to the sent messages and the turn (see `turn_keys`), and other
//! keys reach the text box as usual. Taken in the capture phase, before the
//! text box's own bindings.

use gpui_kit::{
    Context, Window,
    component::input::{Enter, Escape, IndentInline, MoveDown, MoveUp},
};

use super::ChatView;

impl ChatView {
    pub(super) fn on_menu_up(&mut self, _: &MoveUp, window: &mut Window, cx: &mut Context<Self>) {
        if self.move_menu_choice(-1, cx) || self.recall_older(window, cx) {
            cx.stop_propagation();
        }
    }

    pub(super) fn on_menu_down(
        &mut self,
        _: &MoveDown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.move_menu_choice(1, cx) || self.recall_newer(window, cx) {
            cx.stop_propagation();
        }
    }

    fn move_menu_choice(&mut self, step: isize, cx: &mut Context<Self>) -> bool {
        self.move_model_choice(step, cx) || self.move_slash_choice(step, cx)
    }

    /// Tab completes a command without running it.
    pub(super) fn on_menu_tab(
        &mut self,
        _: &IndentInline,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pick_slash_choice(false, window, cx) {
            cx.stop_propagation();
        }
    }

    /// Enter takes the chosen row, as in the CLI; Shift+Enter still breaks
    /// the line.
    pub(super) fn on_menu_enter(
        &mut self,
        enter: &Enter,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if enter.shift || enter.secondary {
            return;
        }
        if self.pick_model_choice(cx) || self.pick_slash_choice(true, window, cx) {
            cx.stop_propagation();
        }
    }

    /// Escape closes the topmost of the picker, the `/` menu and the usage
    /// and context panels; with none open, it stops the turn.
    pub(super) fn on_menu_escape(
        &mut self,
        _: &Escape,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.close_model_picker(cx)
            || self.dismiss_slash_menu(cx)
            || self.close_usage(cx)
            || self.close_context(cx)
            || self.stop_turn(window, cx)
        {
            cx.stop_propagation();
        }
    }
}
