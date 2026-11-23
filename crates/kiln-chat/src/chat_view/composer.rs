//! The composer: the message box and its strip of mode menu, model menu,
//! skills button, context meter and send/stop button.

use std::time::SystemTime;

use gpui_kit::prelude::*;
use gpui_kit::{
    Anchor, AnyElement, Context, Focusable, Hsla, SharedString, TestSupportExt, WeakEntity, Window,
    component::{
        ActiveTheme, Icon, IconName, Sizable,
        button::{Button, ButtonVariants},
        h_flex,
        input::{OutdentInline, Textarea},
        menu::{DropdownMenu, PopupMenuItem},
        v_flex,
    },
    div, px, white,
};
use kiln_agent::{ModelOption, PermissionMode};

use super::{ChatView, header::reset_label};
use crate::{
    agent_state::{self, MENU_MODES},
    spinner,
    transcript::Status,
};

const SEND_BUTTON_SIZE: f32 = 26.;
const MENU_WIDTH: f32 = 280.;

impl ChatView {
    pub(super) fn render_composer(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let focused = self.composer.read(cx).focus_handle(cx).is_focused(window);
        let view = cx.entity().downgrade();
        let strip = h_flex()
            .gap_1()
            .pl_1()
            .pr_1p5()
            .py_1()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(self.mode_menu(&view))
            .children(self.model_menu(&view))
            .child(self.skills_button(cx))
            .child(div().flex_1())
            .children(self.context_meter(cx))
            .child(self.action_button(cx));
        let menu = self
            .render_model_picker(cx)
            .or_else(|| self.render_slash_menu(cx));
        let theme = cx.theme();
        v_flex()
            .relative()
            .rounded(theme.radius * 2.)
            .border_1()
            .border_color(if focused {
                spinner::accent()
            } else {
                theme.border
            })
            .bg(theme.background)
            .capture_action(cx.listener(Self::on_outdent))
            .capture_action(cx.listener(Self::on_menu_up))
            .capture_action(cx.listener(Self::on_menu_down))
            .capture_action(cx.listener(Self::on_menu_tab))
            .capture_action(cx.listener(Self::on_menu_enter))
            .capture_action(cx.listener(Self::on_menu_escape))
            .children(menu)
            .child(
                div()
                    .id("chat-composer")
                    .test_support()
                    .px_1()
                    .pt_1()
                    .child(Textarea::new(&self.composer).appearance(false)),
            )
            .child(strip)
            .into_any_element()
    }

    /// Shift+Tab cycles the mode, as in the CLI. The text box binds it to
    /// outdent, so the action is taken in the capture phase, before it.
    fn on_outdent(&mut self, _: &OutdentInline, _: &mut Window, cx: &mut Context<Self>) {
        self.cycle_mode(cx);
        cx.stop_propagation();
    }

    fn mode_menu(&self, view: &WeakEntity<Self>) -> impl IntoElement {
        let current = self.agent.mode();
        let view = view.clone();
        Button::new("chat-mode")
            .ghost()
            .xsmall()
            .icon(Icon::new(gpui_kit::assets::IconName::Pencil))
            .label(agent_state::mode_label(current))
            .tooltip("Change mode (Shift+Tab)")
            .dropdown_menu_with_anchor(Anchor::BottomLeft, move |menu, _, _| {
                MENU_MODES
                    .into_iter()
                    .fold(menu.min_w(px(MENU_WIDTH)), |menu, mode| {
                        menu.item(mode_item(mode, mode == current, &view))
                    })
            })
    }

    fn model_menu(&self, view: &WeakEntity<Self>) -> Option<impl IntoElement> {
        let label = self.agent.model_label()?;
        let selected = self.agent.selected_model().map(|option| option.id.clone());
        let options = self.agent.models().to_vec();
        let view = view.clone();
        Some(
            Button::new("chat-model")
                .ghost()
                .xsmall()
                .label(label)
                .tooltip("Change model")
                .dropdown_menu_with_anchor(Anchor::BottomLeft, move |menu, _, _| {
                    let menu = menu.min_w(px(MENU_WIDTH)).max_h(px(420.)).scrollable(true);
                    options.iter().fold(menu, |menu, option| {
                        let checked = selected.as_deref() == Some(option.id.as_str());
                        menu.item(model_item(option, checked, &view))
                    })
                }),
        )
    }

    fn action_button(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.status() == Status::Running {
            round_button(
                "chat-stop",
                IconName::Square,
                cx.theme().muted_foreground,
                cx,
            )
            .on_click(cx.listener(|this, _, window, cx| this.interrupt(window, cx)))
            .into_any_element()
        } else if self.is_limited() {
            round_button("chat-send", IconName::ArrowUp, cx.theme().muted, cx).into_any_element()
        } else {
            round_button("chat-send", IconName::ArrowUp, spinner::accent(), cx)
                .on_click(cx.listener(|this, _, window, cx| this.send(window, cx)))
                .into_any_element()
        }
    }

    /// Shown above the composer while a usage limit stops messages; the
    /// conversation stays readable.
    pub(super) fn render_limit_notice(&self, cx: &Context<Self>) -> Option<impl IntoElement> {
        let now = SystemTime::now();
        let limited = self.agent.limit_at(now)?;
        let text = match limited.until {
            Some(until) => format!("Usage limit reached · resets {}", reset_label(until, now)),
            None => "Usage limit reached".to_owned(),
        };
        Some(
            div()
                .id("chat-limit")
                .test_support()
                .aria_label(SharedString::from(text.clone()))
                .mb_2()
                .px_3()
                .py_1p5()
                .rounded(cx.theme().radius * 2.)
                .border_1()
                .border_color(cx.theme().warning)
                .text_sm()
                .child(text),
        )
    }
}

fn mode_item(mode: PermissionMode, checked: bool, view: &WeakEntity<ChatView>) -> PopupMenuItem {
    let view = view.clone();
    let id = SharedString::from(format!("chat-mode-{mode:?}"));
    let (name, description) = (
        agent_state::mode_label(mode),
        agent_state::mode_description(mode),
    );
    PopupMenuItem::element(move |_, cx| {
        option_row(
            id.clone(),
            name.into(),
            description.into(),
            cx.theme().muted_foreground,
        )
    })
    .checked(checked)
    .on_click(move |_, _, cx| {
        let _ = view.update(cx, |view, cx| view.set_mode(mode, cx));
    })
}

fn model_item(option: &ModelOption, checked: bool, view: &WeakEntity<ChatView>) -> PopupMenuItem {
    let view = view.clone();
    let model = option.id.clone();
    let id = SharedString::from(format!("chat-model-{model}"));
    let name = SharedString::from(option.name.clone());
    let description = SharedString::from(option.description.clone());
    PopupMenuItem::element(move |_, cx| {
        option_row(
            id.clone(),
            name.clone(),
            description.clone(),
            cx.theme().muted_foreground,
        )
    })
    .checked(checked)
    .on_click(move |_, _, cx| {
        let _ = view.update(cx, |view, cx| view.set_model(&model, cx));
    })
}

/// A menu option: its name over a one-line description.
pub(super) fn option_row(
    id: SharedString,
    name: SharedString,
    description: SharedString,
    muted: Hsla,
) -> impl IntoElement {
    v_flex()
        .id(id)
        .test_support()
        .aria_label(name.clone())
        .py_1()
        .child(name)
        .when(!description.is_empty(), |this| {
            this.child(div().text_xs().text_color(muted).child(description))
        })
}

fn round_button(
    id: &'static str,
    icon: IconName,
    background: Hsla,
    cx: &Context<ChatView>,
) -> impl StatefulInteractiveElement + IntoElement {
    div()
        .id(id)
        .test_support()
        .flex()
        .items_center()
        .justify_center()
        .size(px(SEND_BUTTON_SIZE))
        .rounded(cx.theme().radius)
        .bg(background)
        .cursor_pointer()
        .child(Icon::new(icon).small().text_color(white()))
}
