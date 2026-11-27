//! What a click on the agent map opens over it: the agent's facts, what it
//! does now, what it was asked and its tool calls, with ways to its
//! transcript and to stop it; for the conversation, its agents by state.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, Context, Div, FontWeight, SharedString, TestSupportExt,
    assets::IconName as Lucide,
    component::{
        ActiveTheme, Disableable as _,
        button::{Button, ButtonVariants as _},
        h_flex,
        text::TextView,
        v_flex,
    },
    div, px,
};
use vortex_agent::ToolCall;

use super::{
    AgentsDialog, ChatView, Page,
    state::{State, main_state},
};
use crate::{
    literal::literal_markdown,
    scale::{ScaledButton as _, ScaledText as _},
    subagent_view, tool_format,
    transcript::{EntryId, Step, StopAttempt, Subagent, Transcript},
};

const WIDTH: f32 = 420.;

/// Which of an agent card's sections are open.
#[derive(Clone, Copy)]
pub struct Sections {
    /// Until the user folds it, the prompt is open while the agent works.
    pub prompt: Option<bool>,
    pub calls: bool,
}

impl Default for Sections {
    fn default() -> Self {
        Self {
            prompt: None,
            calls: true,
        }
    }
}

/// The card of agent `id`, `None` for the conversation itself.
pub fn render(
    id: Option<EntryId>,
    chat: &ChatView,
    sections: Sections,
    cx: &Context<AgentsDialog>,
) -> Option<AnyElement> {
    let body = match id {
        None => main_card(chat, cx),
        Some(id) => agent_card(chat.transcript.subagents().get(id)?, chat, sections, cx),
    };
    let theme = cx.theme();
    let card = body
        .id("chat-agents-card")
        .test_support()
        .occlude()
        .w(px(WIDTH))
        .max_w_full()
        .max_h_full()
        .overflow_y_scroll()
        .p_4()
        .gap_2()
        .rounded(theme.radius_lg)
        .border_1()
        .border_color(theme.border)
        .bg(theme.popover)
        .shadow_lg();
    Some(
        div()
            .id("chat-agents-card-backdrop")
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .p_4()
            .rounded(theme.radius_lg)
            .bg(theme.overlay)
            .on_click(cx.listener(|this, _, window, cx| this.back(window, cx)))
            .child(card)
            .into_any_element(),
    )
}

fn main_card(chat: &ChatView, cx: &Context<AgentsDialog>) -> Div {
    let transcript = &chat.transcript;
    let agents = transcript.subagents();
    let title = chat.title().unwrap_or("This conversation").to_owned();
    let state = main_state(chat);
    let context = chat.agent.context().map(|context| {
        let used = crate::agent_state::tokens(context.used_tokens);
        format!("{used} tokens in context")
    });
    let facts = std::iter::once(state.label().to_owned())
        .chain(chat.agent.model_label())
        .chain(context);
    let noun = if agents.len() == 1 { "agent" } else { "agents" };
    let counts = State::ALL.into_iter().filter_map(|state| {
        let count = (agents.iter())
            .filter(|agent| State::of(agent, transcript) == state)
            .count();
        (count > 0).then(|| muted(format!("{count} {}", state.label().to_lowercase()), cx))
    });
    v_flex()
        .child(title_bar(title, state, cx))
        .child(muted(facts.collect::<Vec<_>>().join(" · "), cx))
        .child(format!("{} {noun}", agents.len()))
        .children(counts.collect::<Vec<_>>())
}

fn agent_card(
    agent: &Subagent,
    chat: &ChatView,
    sections: Sections,
    cx: &Context<AgentsDialog>,
) -> Div {
    let transcript = &chat.transcript;
    let state = State::of(agent, transcript);
    let running = agent.end.is_none();
    let calls: Vec<_> = calls(agent).collect();
    let call_rows: Vec<_> = (calls.iter())
        .map(|(call, _)| call_row(call, chat, cx))
        .collect();
    let prompt = agent.call.input.get("prompt").and_then(|it| it.as_str());
    let prompt_open = sections.prompt.unwrap_or(running);
    v_flex()
        .child(title_bar(agent.description.clone(), state, cx))
        .child(muted(facts(agent, state), cx))
        .child(muted(started_by(agent, transcript), cx))
        .children(usage(agent).map(|usage| muted(usage, cx)))
        .children(activity(agent, chat, cx))
        .children(report(agent, cx))
        .children(prompt.map(|prompt| {
            let text = TextView::markdown("chat-agents-card-prompt-text", literal_markdown(prompt));
            section("chat-agents-card-prompt", "Prompt".into(), prompt_open, cx)
                .when(prompt_open, |this| {
                    this.child(quoted(cx).child(text.selectable(true)))
                })
        }))
        .child({
            let title = format!("Tool calls ({})", calls.len());
            section("chat-agents-card-calls", title, sections.calls, cx)
                .when(sections.calls, |this| this.children(call_rows))
        })
        .child(actions(agent, cx))
        .when(agent.stop == Some(StopAttempt::Failed), |this| {
            this.child(
                div()
                    .text_color(cx.theme().danger)
                    .child("The agent could not be stopped. It may have finished already."),
            )
        })
}

fn title_bar(title: String, state: State, cx: &Context<AgentsDialog>) -> Div {
    let title = SharedString::from(title);
    h_flex()
        .gap_2()
        .child(
            Button::new("chat-agents-card-back")
                .ghost()
                .row_icon(Lucide::ArrowLeft)
                .tooltip("Back to the Map")
                .on_click(cx.listener(|this, _, window, cx| this.back(window, cx))),
        )
        .child(state.dot(cx))
        .child(
            div()
                .id("chat-agents-card-title")
                .test_support()
                .aria_label(title.clone())
                .flex_1()
                .min_w_0()
                .truncate()
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        )
}

fn muted(text: String, cx: &Context<AgentsDialog>) -> Div {
    div()
        .text_caption()
        .text_color(cx.theme().muted_foreground)
        .child(text)
}

fn facts(agent: &Subagent, state: State) -> String {
    let background = agent.call.input.get("run_in_background");
    let place = background
        .and_then(serde_json::Value::as_bool)
        .map(|background| {
            if background {
                "background"
            } else {
                "foreground"
            }
        });
    std::iter::once(state.label().to_owned())
        .chain(subagent_view::identity(agent))
        .chain(place.map(str::to_owned))
        .collect::<Vec<_>>()
        .join(" · ")
}

fn started_by(agent: &Subagent, transcript: &Transcript) -> String {
    let parent = (agent.parent).and_then(|id| transcript.subagents().get(id));
    let name = parent.map_or("this conversation", |parent| parent.description.as_str());
    format!("Started by {name}")
}

fn usage(agent: &Subagent) -> Option<String> {
    let usage = agent.usage.as_ref()?;
    Some(subagent_view::usage_parts(usage).join(" · "))
}

/// Its tool calls, each with whether it is still running.
fn calls(agent: &Subagent) -> impl Iterator<Item = (&ToolCall, bool)> {
    agent.steps.iter().filter_map(|step| match step {
        Step::Tool { call, result } => Some((call, result.is_none())),
        _ => None,
    })
}

/// What it waits for or does now; `None` once it ended.
fn activity(agent: &Subagent, chat: &ChatView, cx: &Context<AgentsDialog>) -> Option<Div> {
    if agent.end.is_some() {
        return None;
    }
    if let Some(tool) = chat.transcript.waits_for(agent) {
        let text = format!(
            "Waiting for your permission to use {}. Answer it in the chat.",
            tool_format::title(tool)
        );
        return Some(div().text_color(cx.theme().warning).child(text));
    }
    let now = calls(agent).filter(|(_, running)| *running).last();
    let Some((call, _)) = now else {
        let text = agent.activity.clone()?;
        return Some(div().child(text));
    };
    Some(
        h_flex()
            .gap_2()
            .child(
                div()
                    .flex_none()
                    .text_color(cx.theme().muted_foreground)
                    .child("Now"),
            )
            .child(call_row(call, chat, cx).flex_1()),
    )
}

fn report(agent: &Subagent, cx: &Context<AgentsDialog>) -> Option<Div> {
    let summary = agent.summary.clone()?;
    let failed = agent.end == Some(vortex_agent::SubagentEnd::Failed);
    Some(
        div()
            .when(failed, |this| this.text_color(cx.theme().danger))
            .child(TextView::markdown("chat-agents-card-report", summary).selectable(true)),
    )
}

fn section(id: &'static str, title: String, open: bool, cx: &Context<AgentsDialog>) -> Div {
    let icon = if open {
        Lucide::ChevronDown
    } else {
        Lucide::ChevronRight
    };
    v_flex().gap_1().child(
        h_flex().child(
            Button::new(id)
                .ghost()
                .row_labelled(icon, title)
                .on_click(cx.listener(move |this, _, _, cx| this.fold(id, !open, cx))),
        ),
    )
}

fn quoted(cx: &Context<AgentsDialog>) -> Div {
    let theme = cx.theme();
    div()
        .p_2()
        .rounded(theme.radius)
        .border_1()
        .border_color(theme.border)
        .bg(theme.secondary)
}

fn call_row(call: &ToolCall, chat: &ChatView, cx: &Context<AgentsDialog>) -> Div {
    h_flex()
        .min_w_0()
        .gap_2()
        .child(
            div()
                .flex_none()
                .font_weight(FontWeight::SEMIBOLD)
                .child(tool_format::title(&call.name)),
        )
        .child(
            div()
                .min_w_0()
                .truncate()
                .font_family(cx.theme().mono_font_family.clone())
                .child(tool_format::detail(&call.input, &chat.cwd)),
        )
}

fn actions(agent: &Subagent, cx: &Context<AgentsDialog>) -> Div {
    let id = agent.id;
    h_flex()
        .pt_2()
        .gap_2()
        .child(
            Button::new("chat-agents-card-transcript")
                .outline()
                .control()
                .flex_1()
                .label("Open transcript")
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.go(Page::Transcript(id), window, cx);
                })),
        )
        .children(
            stop_button("chat-agents-card-stop", agent, cx)
                .map(|button| button.outline().flex_1().text_color(cx.theme().danger)),
        )
}

/// Stops the agent; `None` once it ended.
pub fn stop_button(
    id: &'static str,
    agent: &Subagent,
    cx: &Context<AgentsDialog>,
) -> Option<Button> {
    agent.end.is_none().then(|| {
        let stopping = agent.stop == Some(StopAttempt::Asked);
        let agent = agent.id;
        Button::new(id)
            .control()
            .label(if stopping {
                "Stopping…"
            } else {
                "Stop agent"
            })
            .disabled(stopping)
            .on_click(cx.listener(move |this, _, _, cx| this.stop(agent, cx)))
    })
}
