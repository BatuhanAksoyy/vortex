//! The `/` command menu above the composer. It opens while the first word
//! of the message is a `/name` being typed, lists the commands the agent
//! offers that match, and is driven from the keyboard without leaving the
//! text box (see `menu_keys`): Up/Down choose, Tab completes, Enter runs,
//! Escape closes.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, Context, ScrollHandle, SharedString, TestSupportExt, Window,
    component::{ActiveTheme, ThemeStyled as _, h_flex, v_flex},
    div, px,
};
use kiln_agent::SlashCommand;

use super::ChatView;
use crate::slash;

const MENU_MAX_HEIGHT: f32 = 280.;
/// Some hints spell out every flag; the description deserves the room.
const HINT_MAX_WIDTH: f32 = 220.;

/// What the menu shows and which row is chosen. The name typed is kept so
/// the choice resets only when it changes.
#[derive(Default)]
pub(super) struct SlashMenu {
    typed: Option<String>,
    selected: usize,
    /// Escape closes the menu until a different name is typed.
    dismissed: bool,
    scroll: ScrollHandle,
}

impl ChatView {
    /// Follows the composer: called whenever its text changes.
    pub(super) fn sync_slash_menu(&mut self, cx: &mut Context<Self>) {
        let composer = self.composer.read(cx);
        let typed = slash::typed_name(&composer.value(), composer.cursor()).map(str::to_owned);
        if typed != self.slash.typed {
            self.slash.typed = typed;
            self.slash.selected = 0;
            self.slash.dismissed = false;
            self.slash.scroll.scroll_to_item(0);
            cx.notify();
        }
    }

    fn slash_matches(&self) -> Vec<&SlashCommand> {
        match &self.slash.typed {
            Some(typed) if !self.slash.dismissed => slash::matching(self.agent.commands(), typed),
            _ => Vec::new(),
        }
    }

    fn selected_command(&self) -> Option<SlashCommand> {
        self.slash_matches()
            .get(self.slash.selected)
            .map(|command| (*command).clone())
    }

    /// Moves the choice by `step` rows; `false` when the menu is closed.
    pub(super) fn move_slash_choice(&mut self, step: isize, cx: &mut Context<Self>) -> bool {
        let count = self.slash_matches().len();
        if count == 0 {
            return false;
        }
        self.slash.selected = wrap(self.slash.selected, step, count);
        self.slash.scroll.scroll_to_item(self.slash.selected);
        cx.notify();
        true
    }

    /// Completes the chosen command, and runs it if `run`; `false` when the
    /// menu is closed.
    pub(super) fn pick_slash_choice(
        &mut self,
        run: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(command) = self.selected_command() else {
            return false;
        };
        self.pick_command(&command, run, window, cx);
        true
    }

    /// Closes the menu until a different name is typed; `false` when it is
    /// already closed.
    pub(super) fn dismiss_slash_menu(&mut self, cx: &mut Context<Self>) -> bool {
        if self.slash_matches().is_empty() {
            return false;
        }
        self.slash.dismissed = true;
        cx.notify();
        true
    }

    /// Puts `/name` in the composer, then sends it if `run`.
    fn pick_command(
        &mut self,
        command: &SlashCommand,
        run: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (text, cursor) = slash::complete(&self.composer.read(cx).value(), command);
        self.composer.update(cx, |composer, cx| {
            composer.replace_all(text, window, cx);
            composer.set_selected_range(cursor..cursor, cx);
            composer.focus(window, cx);
        });
        self.sync_slash_menu(cx);
        if run {
            self.send(window, cx);
        }
    }

    pub(super) fn render_slash_menu(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let matches = self.slash_matches();
        if matches.is_empty() {
            return None;
        }
        let rows = matches
            .iter()
            .enumerate()
            .map(|(index, command)| Self::slash_row(command, index == self.slash.selected, cx));
        Some(
            menu_surface("chat-slash-menu", &self.slash.scroll, cx)
                .children(rows)
                .into_any_element(),
        )
    }

    fn slash_row(
        command: &SlashCommand,
        selected: bool,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let name = SharedString::from(format!("/{}", command.name));
        let picked = command.clone();
        let id = SharedString::from(format!("chat-slash-{}", command.name));
        menu_row(id, name.clone(), selected, cx)
            .child(div().flex_none().child(name))
            .when(!command.argument_hint.is_empty(), |this| {
                this.child(
                    div()
                        .flex_none()
                        .max_w(px(HINT_MAX_WIDTH))
                        .truncate()
                        .text_color(theme.muted_foreground)
                        .child(command.argument_hint.clone()),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(command.description.clone()),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.pick_command(&picked, false, window, cx);
            }))
    }
}

/// The floating surface of a composer menu: above the composer, as wide as
/// it, over the transcript.
pub(super) fn menu_surface(
    id: &'static str,
    scroll: &ScrollHandle,
    cx: &Context<ChatView>,
) -> impl ParentElement + IntoElement + use<> {
    v_flex()
        .id(id)
        .test_support()
        .absolute()
        .bottom_full()
        .left_0()
        .right_0()
        .mb_1()
        .p_1()
        .max_h(px(MENU_MAX_HEIGHT))
        .overflow_y_scroll()
        .track_scroll(scroll)
        .occlude()
        .popover_style(cx)
}

/// A row of a composer menu, highlighted while chosen.
pub(super) fn menu_row(
    id: SharedString,
    label: SharedString,
    selected: bool,
    cx: &Context<ChatView>,
) -> impl ParentElement + StatefulInteractiveElement + IntoElement + use<> {
    let theme = cx.theme();
    h_flex()
        .id(id)
        .test_support()
        .aria_label(label)
        .gap_2()
        .px_2()
        .py_1()
        .rounded(theme.radius)
        .text_sm()
        .cursor_pointer()
        .when(selected, |this| {
            this.bg(theme.accent).text_color(theme.accent_foreground)
        })
        .when(!selected, |this| this.hover(|this| this.bg(theme.accent)))
}

/// The row `step` rows from `selected` among `count`, wrapping around.
pub(super) fn wrap(selected: usize, step: isize, count: usize) -> usize {
    selected
        .checked_add_signed(step)
        .filter(|selected| *selected < count)
        .unwrap_or(if step < 0 { count - 1 } else { 0 })
}
