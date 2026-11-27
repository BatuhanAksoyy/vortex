//! The subagent picked in the subagents view: its facts, then its chat.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Context, FontWeight, SharedString, TestSupportExt, WeakEntity,
    component::{
        ActiveTheme, Sizable as _,
        button::{Button, ButtonVariants as _},
        h_flex,
        text::TextView,
        v_flex,
    },
    div, px,
};
use vortex_agent::SubagentEnd;

use super::{AgentsDialog, ChatView};
use crate::{
    agent_state::tokens,
    literal::literal_markdown,
    scale::{ScaledButton as _, ScaledText as _},
    subagent_view::{self, StepPlace, small_dot},
    transcript::{EntryId, Subagent, Subagents},
};

pub fn render(
    agent: &Subagent,
    chat: &ChatView,
    weak_chat: &WeakEntity<ChatView>,
    cx: &Context<AgentsDialog>,
) -> AnyElement {
    let id = agent.id;
    let agents = chat.transcript.subagents();
    let inner = |id| inner_row(id, agents, cx);
    let place = StepPlace {
        root: &chat.cwd,
        chat: weak_chat,
        inner: &inner,
    };
    let steps: Vec<_> = (agent.steps.iter().enumerate())
        .map(|(index, step)| {
            let step_id = SharedString::from(format!("chat-agent-step-{id}-{index}"));
            subagent_view::step_row(step, step_id, &place, cx)
        })
        .collect();
    v_flex()
        .id("chat-agent-detail")
        .test_support()
        .aria_label(SharedString::from(agent.description.clone()))
        .flex_1()
        .min_w_0()
        .h_full()
        .overflow_y_scroll()
        .p_3()
        .gap_3()
        .children(above(agent, agents, cx))
        .child(title(agent, cx))
        .child(facts(agent, agents, cx))
        .child(div().h_px().bg(cx.theme().border))
        .children(prompt(agent, cx))
        .children(steps)
        .children(report(agent, cx))
        .into_any_element()
}

fn title(agent: &Subagent, cx: &Context<AgentsDialog>) -> impl IntoElement + use<> {
    let id = agent.id;
    h_flex()
        .items_start()
        .gap_2()
        .pr_6()
        .child(small_dot(agent.end, cx))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .font_weight(FontWeight::SEMIBOLD)
                .child(agent.description.clone()),
        )
        .children(super::card::stop_button("chat-agent-stop", agent, cx).map(Button::ghost))
        .child(
            Button::new("chat-agent-reveal")
                .ghost()
                .control()
                .label("Show in chat")
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.show_in_chat(id, window, cx);
                })),
        )
}

/// The subagents it is below, each a way back up; `None` for the main
/// agent's.
fn above(
    agent: &Subagent,
    agents: &Subagents,
    cx: &Context<AgentsDialog>,
) -> Option<impl IntoElement + use<>> {
    agent.parent?;
    let path = agents.path(agent.id);
    let above = path.iter().filter(|it| it.id != agent.id).map(|it| {
        let id = it.id;
        h_flex()
            .gap_1()
            .min_w_0()
            .child(
                Button::new(SharedString::from(format!("chat-agent-above-{id}")))
                    .ghost()
                    .small()
                    .px_1()
                    .label(it.description.clone())
                    .on_click(cx.listener(move |this, _, _, cx| this.show(id, cx))),
            )
            .child("›")
    });
    Some(
        h_flex()
            .flex_wrap()
            .gap_1()
            .pr_6()
            .text_color(cx.theme().muted_foreground)
            .children(above.collect::<Vec<_>>()),
    )
}

/// A subagent it started, which a click shows.
fn inner_row(id: EntryId, agents: &Subagents, cx: &Context<AgentsDialog>) -> AnyElement {
    let Some(agent) = agents.get(id) else {
        return div().into_any_element();
    };
    let theme = cx.theme();
    let hover = theme.secondary;
    h_flex()
        .id(SharedString::from(format!("chat-agent-inner-{id}")))
        .test_support()
        .aria_label(SharedString::from(agent.description.clone()))
        .items_start()
        .gap_2()
        .px_2()
        .py_1()
        .rounded(theme.radius)
        .border_1()
        .border_color(theme.border)
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .on_click(cx.listener(move |this, _, _, cx| this.show(id, cx)))
        .child(small_dot(agent.end, cx))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(div().truncate().child(agent.description.clone()))
                .child(
                    div()
                        .truncate()
                        .text_caption()
                        .text_color(theme.muted_foreground)
                        .child(subagent_view::full_status(agent, agents)),
                ),
        )
        .child(div().text_color(theme.muted_foreground).child("›"))
        .into_any_element()
}

fn facts(agent: &Subagent, agents: &Subagents, cx: &App) -> impl IntoElement + use<> {
    let context = agent.request.map_or_else(
        || "Not reported yet".to_owned(),
        |request| {
            let total = request.cached + request.written + request.uncached;
            format!(
                "{} tokens in the last request · {} read from cache",
                tokens(total),
                tokens(request.cached)
            )
        },
    );
    let fact = |name: &'static str, value: String| {
        h_flex()
            .id(SharedString::from(format!(
                "chat-agent-{}",
                name.to_lowercase()
            )))
            .test_support()
            .aria_label(SharedString::from(value.clone()))
            .gap_2()
            .child(
                div()
                    .flex_none()
                    .w(px(72.))
                    .text_color(cx.theme().muted_foreground)
                    .child(name),
            )
            .child(div().min_w_0().child(value))
    };
    let unknown = || "Not reported yet".to_owned();
    v_flex()
        .gap_1()
        .child(fact("Status", subagent_view::full_status(agent, agents)))
        .child(fact("Type", agent.kind.clone().unwrap_or_else(unknown)))
        .child(fact("Model", agent.model.clone().unwrap_or_else(unknown)))
        .child(fact("Context", context))
}

/// What it was asked, as the chat's first message.
fn prompt(agent: &Subagent, cx: &App) -> Option<impl IntoElement + use<>> {
    let text = agent.call.input.get("prompt")?.as_str()?;
    let theme = cx.theme();
    Some(
        div()
            .px_3()
            .py_1p5()
            .rounded(theme.radius * 1.5)
            .border_1()
            .border_color(theme.border)
            .bg(theme.secondary)
            .child(
                TextView::markdown(("chat-agent-prompt", agent.id), literal_markdown(text))
                    .selectable(true),
            ),
    )
}

fn report(agent: &Subagent, cx: &App) -> Option<impl IntoElement + use<>> {
    let summary = agent.summary.clone()?;
    let failed = agent.end != Some(SubagentEnd::Completed);
    let theme = cx.theme();
    Some(
        v_flex()
            .gap_1()
            .pt_1()
            .border_t_1()
            .border_color(theme.border)
            .child(
                div()
                    .text_caption()
                    .text_color(theme.muted_foreground)
                    .child(if failed { "Why it ended" } else { "Report" }),
            )
            .child(TextView::markdown(("chat-agent-report", agent.id), summary).selectable(true)),
    )
}
