//! The permission question that replaces the composer while the agent waits:
//! numbered options answered by click, number key or arrows + Enter, and a
//! field to tell the agent what to do instead.

use std::path::Path;

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, Context, EventEmitter, FocusHandle, Focusable, FontWeight, KeyDownEvent,
    SharedString, Subscription, TestSupportExt, Window,
    component::{
        ActiveTheme, h_flex,
        input::{Input, InputEvent, InputState},
        v_flex,
    },
    div, px,
};
use kiln_agent::{Decision, PermissionRequest, Remember, RuleScope};

use crate::{spinner, tool_format};

const DETAIL_MAX_HEIGHT: f32 = 240.;

pub struct Answered {
    pub request_id: String,
    pub decision: Decision,
}

type PromptOption = (&'static str, &'static str, Decision);

pub struct PermissionPrompt {
    request: PermissionRequest,
    title: String,
    detail: String,
    selected: usize,
    feedback: gpui_kit::Entity<InputState>,
    focus: FocusHandle,
    _subscription: Subscription,
}

impl EventEmitter<Answered> for PermissionPrompt {}

impl PermissionPrompt {
    pub fn new(
        request: PermissionRequest,
        root: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let feedback =
            cx.new(|cx| InputState::new(window, cx).placeholder("Tell Claude what to do instead"));
        let subscription = cx.subscribe_in(&feedback, window, |this, input, event, _, cx| {
            if let InputEvent::PressEnter { .. } = event {
                let text = input.read(cx).value().trim().to_owned();
                let feedback = (!text.is_empty()).then_some(text);
                this.answer(Decision::Deny { feedback }, cx);
            }
        });
        let (title, detail) = describe(&request, root);
        Self {
            request,
            title,
            detail,
            selected: 0,
            feedback,
            focus: cx.focus_handle(),
            _subscription: subscription,
        }
    }

    fn options(&self) -> Vec<PromptOption> {
        let always = |scope| Decision::AllowAlways(scope);
        let mut options = vec![("permission-yes", "Yes", Decision::Allow)];
        match self.request.remember {
            Some(Remember::Rules(_)) => options.extend([
                (
                    "permission-session",
                    "Yes, for this session",
                    always(RuleScope::Session),
                ),
                (
                    "permission-always",
                    "Yes, and don't ask again in this project",
                    always(RuleScope::Project),
                ),
                (
                    "permission-everywhere",
                    "Yes, and don't ask again in any project",
                    always(RuleScope::User),
                ),
            ]),
            Some(Remember::AcceptEdits) => options.push((
                "permission-always",
                "Yes, and auto-accept edits",
                always(RuleScope::Session),
            )),
            None => {}
        }
        options.push(("permission-no", "No", Decision::Deny { feedback: None }));
        options
    }

    /// The rules "don't ask again" would add, e.g. `Bash(npm test:*)`.
    fn rules(&self) -> Option<String> {
        match &self.request.remember {
            Some(Remember::Rules(names)) if !names.is_empty() => Some(names.join(", ")),
            _ => None,
        }
    }

    fn answer(&mut self, decision: Decision, cx: &mut Context<Self>) {
        cx.emit(Answered {
            request_id: self.request.id.clone(),
            decision,
        });
    }

    fn on_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let feedback_focus = self.feedback.read(cx).focus_handle(cx);
        if feedback_focus.is_focused(window) {
            if event.keystroke.key == "escape" {
                window.focus(&self.focus, cx);
                cx.stop_propagation();
            }
            return;
        }
        let options = self.options();
        let key = event.keystroke.key.as_str();
        let chosen = match key {
            "enter" => options.get(self.selected),
            "escape" => options.last(),
            _ => key
                .parse::<usize>()
                .ok()
                .and_then(|n| options.get(n.checked_sub(1)?)),
        };
        if let Some((_, _, decision)) = chosen {
            self.answer(decision.clone(), cx);
        } else if key == "up" {
            self.selected = self.selected.saturating_sub(1);
        } else if key == "down" && self.selected + 1 < options.len() {
            self.selected += 1;
        } else if key == "down" {
            window.focus(&feedback_focus, cx);
        } else {
            return;
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn option_row(&self, index: usize, option: PromptOption, cx: &mut Context<Self>) -> AnyElement {
        let (id, label, decision) = option;
        let theme = cx.theme();
        let selected = index == self.selected;
        let (background, foreground) = if selected {
            (spinner::accent(), theme.primary_foreground)
        } else {
            (theme.background, theme.foreground)
        };
        h_flex()
            .id(id)
            .test_support()
            .gap_3()
            .px_3()
            .py_1p5()
            .rounded(theme.radius)
            .border_1()
            .border_color(if selected {
                spinner::accent()
            } else {
                theme.border
            })
            .bg(background)
            .text_color(foreground)
            .cursor_pointer()
            .child(div().opacity(0.7).child((index + 1).to_string()))
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .child(SharedString::from(label)),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.answer(decision.clone(), cx)))
            .into_any_element()
    }
}

/// The question and the argument it is about, worded by tool.
fn describe(request: &PermissionRequest, root: &Path) -> (String, String) {
    let detail = tool_format::detail(&request.input, root);
    let file = tool_format::file_name(&request.input);
    match (request.tool.as_str(), file) {
        ("Edit" | "MultiEdit" | "Write" | "NotebookEdit", Some(file)) => {
            (format!("Make this edit to {file}?"), String::new())
        }
        ("Bash", _) => {
            let command = request
                .input
                .get("command")
                .and_then(serde_json::Value::as_str);
            (
                "Allow Claude to run this command?".to_owned(),
                command.unwrap_or(&detail).to_owned(),
            )
        }
        ("ExitPlanMode", _) => {
            let plan = request
                .input
                .get("plan")
                .and_then(serde_json::Value::as_str);
            (
                "Claude's plan is ready. Start implementing?".to_owned(),
                plan.unwrap_or_default().to_owned(),
            )
        }
        (tool, _) => (
            format!("Allow Claude to use {}?", tool_format::title(tool)),
            detail,
        ),
    }
}

impl Focusable for PermissionPrompt {
    fn focus_handle(&self, _: &gpui_kit::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl PermissionPrompt {
    /// What the call would do: the command, or Claude's plan; scrolls when
    /// long so the options stay in view.
    fn render_detail(&self, cx: &gpui_kit::App) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .id("permission-detail")
            .max_h(px(DETAIL_MAX_HEIGHT))
            .overflow_y_scroll()
            .px_2()
            .py_1()
            .rounded(theme.radius)
            .bg(theme.background)
            .text_sm()
            .font_family(theme.mono_font_family.clone())
            .child(self.detail.clone())
    }
}

impl Render for PermissionPrompt {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let options: Vec<AnyElement> = self
            .options()
            .into_iter()
            .enumerate()
            .map(|(index, option)| self.option_row(index, option, cx))
            .collect();
        let theme = cx.theme();
        v_flex()
            .id("permission-prompt")
            .test_support()
            .aria_label("Permission prompt")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .gap_2()
            .p_3()
            .rounded(theme.radius * 2.)
            .border_1()
            .border_color(spinner::accent())
            .bg(theme.secondary)
            .child(
                div()
                    .id("permission-title")
                    .test_support()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.title.clone()),
            )
            .when(!self.detail.is_empty(), |this| {
                this.child(self.render_detail(cx))
            })
            .when_some(self.request.description.clone(), |this, description| {
                this.child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child(description),
                )
            })
            .when_some(self.rules(), |this, rules| {
                this.child(
                    div()
                        .id("permission-rule")
                        .test_support()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child(format!("Rule: {rules}")),
                )
            })
            .children(options)
            .child(
                div()
                    .id("permission-feedback")
                    .test_support()
                    .px_1()
                    .rounded(theme.radius)
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.background)
                    .child(Input::new(&self.feedback).appearance(false)),
            )
    }
}
