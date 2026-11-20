//! Keys the composer's menus take from the text box while one is open: the
//! model picker first, then the `/` menu. With neither open, the keys reach
//! the text box as usual. Taken in the capture phase, before the text box's
//! own bindings.

use gpui_kit::{
    Context, Window,
    component::input::{Enter, Escape, IndentInline, MoveDown, MoveUp},
};

use super::ChatView;

impl ChatView {
    pub(super) fn on_menu_up(&mut self, _: &MoveUp, _: &mut Window, cx: &mut Context<Self>) {
        self.move_menu_choice(-1, cx);
    }

    pub(super) fn on_menu_down(&mut self, _: &MoveDown, _: &mut Window, cx: &mut Context<Self>) {
        self.move_menu_choice(1, cx);
    }

    fn move_menu_choice(&mut self, step: isize, cx: &mut Context<Self>) {
        if self.move_model_choice(step, cx) || self.move_slash_choice(step, cx) {
            cx.stop_propagation();
        }
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
    /// panel.
    pub(super) fn on_menu_escape(&mut self, _: &Escape, _: &mut Window, cx: &mut Context<Self>) {
        if self.close_model_picker(cx) || self.dismiss_slash_menu(cx) || self.close_usage(cx) {
            cx.stop_propagation();
        }
    }
}
