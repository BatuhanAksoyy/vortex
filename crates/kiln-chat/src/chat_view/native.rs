//! Commands Kiln answers itself (see `slash::NativeCommand`): they never
//! reach the agent as a message, so they work at once, even mid-turn. Also
//! the model picker `/model` opens above the composer.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, Context, ScrollHandle, SharedString, Window,
    component::{ActiveTheme, Icon, IconName, v_flex},
    div,
};
use kiln_agent::{ModelOption, SessionCommand};

use super::{
    ChatView, StripPanel,
    slash_menu::{menu_row, menu_surface, wrap},
};
use crate::{
    scale::{ScaledIcon as _, ScaledText as _},
    slash::NativeCommand,
};

/// The model picker's chosen row.
#[derive(Default)]
pub(super) struct ModelPicker {
    selected: usize,
    scroll: ScrollHandle,
}

impl ChatView {
    pub(super) fn run_native(
        &mut self,
        command: NativeCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match command {
            NativeCommand::Usage => self.set_usage_open(true, cx),
            NativeCommand::Context => self.set_context_open(true, cx),
            NativeCommand::Hooks => self.set_hooks_open(true, window, cx),
            NativeCommand::Model(Some(model)) => self.set_model(&model, cx),
            NativeCommand::Model(None) => self.open_model_picker(cx),
            NativeCommand::Clear => self.new_conversation(window, cx),
        }
    }

    pub(super) fn is_open(&self, panel: StripPanel) -> bool {
        self.open_panel == Some(panel)
    }

    /// Opens `panel`, putting any other away, or closes it; `true` when it
    /// was just opened. Closing a panel that is not open leaves the open one.
    pub(super) fn show_panel(&mut self, panel: StripPanel, open: bool) -> bool {
        let opened = open && !self.is_open(panel);
        if open {
            self.open_panel = Some(panel);
        } else if self.is_open(panel) {
            self.open_panel = None;
        }
        opened
    }

    /// Opens or closes the header's usage panel; opening asks for fresh
    /// usage.
    pub(super) fn set_usage_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.show_panel(StripPanel::Usage, open) {
            self.refresh_usage(cx);
        }
        cx.notify();
    }

    /// Opens or closes the context panel; opening asks for a fresh
    /// breakdown.
    pub(super) fn set_context_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.show_panel(StripPanel::Context, open) {
            self.send_command(SessionCommand::RefreshContext);
        }
        cx.notify();
    }

    /// `false` when the context panel is already closed.
    pub(super) fn close_context(&mut self, cx: &mut Context<Self>) -> bool {
        let was_open = self.is_open(StripPanel::Context);
        self.set_context_open(false, cx);
        was_open
    }

    /// `false` when the usage panel is already closed.
    pub(super) fn close_usage(&mut self, cx: &mut Context<Self>) -> bool {
        let was_open = self.is_open(StripPanel::Usage);
        self.set_usage_open(false, cx);
        was_open
    }

    fn open_model_picker(&mut self, cx: &mut Context<Self>) {
        // The list it opens with may be stale; a fresh one replaces it.
        self.send_command(SessionCommand::RefreshModels);
        let current = self.agent.selected_model().map(|option| option.id.clone());
        let models = self.agent.models();
        if models.is_empty() {
            return;
        }
        let selected = models
            .iter()
            .position(|option| Some(&option.id) == current.as_ref())
            .unwrap_or(0);
        let picker = ModelPicker {
            selected,
            scroll: ScrollHandle::new(),
        };
        picker.scroll.scroll_to_item(selected);
        self.model_picker = Some(picker);
        cx.notify();
    }

    /// `false` when the picker is closed.
    pub(super) fn move_model_choice(&mut self, step: isize, cx: &mut Context<Self>) -> bool {
        let count = self.agent.models().len();
        let Some(picker) = &mut self.model_picker else {
            return false;
        };
        picker.selected = wrap(picker.selected, step, count);
        picker.scroll.scroll_to_item(picker.selected);
        cx.notify();
        true
    }

    /// Switches to the chosen model; `false` when the picker is closed.
    pub(super) fn pick_model_choice(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(picker) = self.model_picker.take() else {
            return false;
        };
        if let Some(option) = self.agent.models().get(picker.selected) {
            let model = option.id.clone();
            self.set_model(&model, cx);
        }
        cx.notify();
        true
    }

    /// `false` when the picker is already closed.
    pub(super) fn close_model_picker(&mut self, cx: &mut Context<Self>) -> bool {
        let was_open = self.model_picker.take().is_some();
        if was_open {
            cx.notify();
        }
        was_open
    }

    pub(super) fn render_model_picker(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let picker = self.model_picker.as_ref()?;
        let current = self.agent.selected_model().map(|option| option.id.clone());
        let rows = self
            .agent
            .models()
            .iter()
            .enumerate()
            .map(|(index, option)| {
                let checked = current.as_deref() == Some(option.id.as_str());
                Self::model_row(option, index == picker.selected, checked, cx)
            });
        let title = div()
            .px_2()
            .py_1()
            .text_caption()
            .text_color(cx.theme().muted_foreground)
            .child("Switch model");
        Some(
            menu_surface("chat-model-picker", &picker.scroll, cx)
                .child(title)
                .children(rows)
                .into_any_element(),
        )
    }

    fn model_row(
        option: &ModelOption,
        selected: bool,
        checked: bool,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let model = option.id.clone();
        let id = SharedString::from(format!("chat-pick-model-{model}"));
        let muted = cx.theme().muted_foreground;
        menu_row(id, option.name.clone().into(), selected, cx)
            .child(v_flex().flex_1().min_w_0().child(option.name.clone()).when(
                !option.description.is_empty(),
                |this| {
                    this.child(
                        div()
                            .truncate()
                            .text_caption()
                            .text_color(muted)
                            .child(option.description.clone()),
                    )
                },
            ))
            .when(checked, |this| {
                this.child(Icon::new(IconName::Check).in_row())
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.model_picker = None;
                this.set_model(&model, cx);
            }))
    }
}
