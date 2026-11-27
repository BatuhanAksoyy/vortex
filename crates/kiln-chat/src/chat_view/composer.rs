//! The composer: the message box and its strip of mode menu, model menu,
//! effort menu, skills and hooks buttons, prompt cache button, context
//! meter and send/stop button.

use std::time::SystemTime;

use gpui_kit::prelude::*;
use gpui_kit::{
    Anchor, AnyElement, Context, Focusable, Hsla, SharedString, TestSupportExt, WeakEntity, Window,
    component::{
        ActiveTheme, Icon, IconName,
        button::{Button, ButtonVariants},
        h_flex,
        input::{OutdentInline, Textarea},
        menu::{DropdownMenu, PopupMenuItem},
        v_flex,
    },
    div, px,
};
use kiln_agent::{Effort, ModelOption, PermissionMode};

use super::ChatView;
use crate::look::Spot;
use crate::{
    agent_state::{self, MENU_MODES},
    clock,
    scale::{ScaledButton as _, ScaledIcon as _, ScaledText as _},
    spinner,
};

const MENU_WIDTH: f32 = 280.;

impl ChatView {
    pub(super) fn render_composer(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let focused = self.composer.read(cx).focus_handle(cx).is_focused(window);
        let view = cx.entity().downgrade();
        let strip = self.strip(cx);
        let paste_view = view.clone();
        let menu = self
            .render_model_picker(cx)
            .or_else(|| self.render_slash_menu(cx));
        let theme = cx.theme();
        v_flex()
            .relative()
            .rounded(theme.radius * 2.)
            .border_1()
            .border_color(if focused {
                spinner::accent(cx)
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
            .children(self.render_attachments(cx))
            .child(
                div()
                    .id("chat-composer")
                    .test_support()
                    .px_1()
                    .pt_1()
                    .child(Textarea::new(&self.composer).appearance(false).on_paste(
                        move |item, _, cx| {
                            paste_view
                                .update(cx, |view, cx| view.paste(item, cx))
                                .unwrap_or(false)
                        },
                    )),
            )
            .child(strip)
            .into_any_element()
    }

    fn strip(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let choices = self.render_spot(Spot::BottomStart, cx);
        h_flex()
            .items_end()
            .gap_1()
            .pl_2()
            .pr_1p5()
            .py_1p5()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(choices)
            .child(self.render_spot(Spot::BottomEnd, cx))
            .child(self.action_button(cx))
    }

    /// Shift+Tab cycles the mode, as in the CLI. The text box binds it to
    /// outdent, so the action is taken in the capture phase, before it.
    fn on_outdent(&mut self, _: &OutdentInline, _: &mut Window, cx: &mut Context<Self>) {
        self.cycle_mode(cx);
        cx.stop_propagation();
    }

    pub(super) fn mode_menu(&self, view: &WeakEntity<Self>) -> impl IntoElement {
        let current = self.agent.mode();
        let view = view.clone();
        Button::new("chat-mode")
            .ghost()
            .control_labelled(
                Icon::new(gpui_kit::assets::IconName::Pencil),
                agent_state::mode_label(current),
            )
            .tooltip("Change mode (Shift+Tab)")
            .dropdown_menu_with_anchor(Anchor::BottomLeft, move |menu, _, _| {
                MENU_MODES
                    .into_iter()
                    .chain([PermissionMode::BypassPermissions])
                    .fold(menu.min_w(px(MENU_WIDTH)), |menu, mode| {
                        menu.item(mode_item(mode, mode == current, &view))
                    })
            })
    }

    pub(super) fn model_menu(&self, view: &WeakEntity<Self>) -> Option<impl IntoElement> {
        let label = self.agent.model_label()?;
        let selected = self.agent.selected_model().map(|option| option.id.clone());
        let options = self.agent.models().to_vec();
        let view = view.clone();
        Some(
            Button::new("chat-model")
                .ghost()
                .control()
                .px_1p5()
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

    /// Shown once the agent names the effort, for a model that takes one.
    pub(super) fn effort_menu(&self, view: &WeakEntity<Self>) -> Option<impl IntoElement> {
        let current = self.agent.effort()?;
        let efforts = self.agent.efforts().to_vec();
        if efforts.is_empty() {
            return None;
        }
        let view = view.clone();
        Some(
            Button::new("chat-effort")
                .ghost()
                .control_labelled(
                    Icon::new(gpui_kit::assets::IconName::Gauge),
                    agent_state::effort_label(current),
                )
                .tooltip("Change effort")
                .dropdown_menu_with_anchor(Anchor::BottomLeft, move |menu, _, _| {
                    efforts
                        .iter()
                        .fold(menu.min_w(px(MENU_WIDTH)), |menu, effort| {
                            menu.item(effort_item(*effort, *effort == current, &view))
                        })
                }),
        )
    }

    fn action_button(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.can_stop(cx) {
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
            round_button("chat-send", IconName::ArrowUp, spinner::accent(cx), cx)
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
            Some(until) => format!(
                "Usage limit reached · resets {}",
                clock::reset_label(until, now)
            ),
            None => "Usage limit reached".to_owned(),
        };
        Some(
            h_flex()
                .id("chat-limit")
                .test_support()
                .aria_label(SharedString::from(text.clone()))
                .flex_wrap()
                .justify_between()
                .gap_2()
                .mb_2()
                .px_3()
                .py_1p5()
                .rounded(cx.theme().radius * 2.)
                .border_1()
                .border_color(cx.theme().warning)
                .text_sm()
                .child(text)
                .children(self.limit_offer(now, cx)),
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
    .on_click(move |_, window, cx| {
        let _ = view.update(cx, |view, cx| view.choose_mode(mode, window, cx));
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

fn effort_item(effort: Effort, checked: bool, view: &WeakEntity<ChatView>) -> PopupMenuItem {
    let view = view.clone();
    let id = SharedString::from(format!("chat-effort-{effort:?}"));
    let (name, description) = (
        agent_state::effort_label(effort),
        agent_state::effort_description(effort),
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
        let _ = view.update(cx, |view, cx| view.set_effort(effort, cx));
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
            this.child(div().text_caption().text_color(muted).child(description))
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
        .size_7()
        .rounded(cx.theme().radius)
        .bg(background)
        .cursor_pointer()
        .child(
            Icon::new(icon)
                .in_control()
                .text_color(crate::spinner::accent_foreground(cx)),
        )
}
