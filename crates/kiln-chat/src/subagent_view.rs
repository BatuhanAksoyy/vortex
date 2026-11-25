//! A subagent's card on the timeline: what it was asked, how far it got and
//! what it used, and, expanded, its own steps and report.

use std::{path::Path, time::Duration};

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, FontWeight, Hsla, SharedString, TestSupportExt, WeakEntity,
    assets::IconName as Lucide,
    component::{
        ActiveTheme, Icon,
        button::{Button, ButtonVariants as _},
        h_flex,
        text::TextView,
        v_flex,
    },
    div, px,
};
use kiln_agent::{SubagentEnd, SubagentUsage};

use crate::{
    ChatView,
    agent_state::tokens,
    entry_view::{EntryContext, sub_line, timeline_row, toggle_card},
    pictures,
    scale::{ScaledButton as _, ScaledIcon as _, ScaledText as _},
    spinner, tool_format,
    transcript::{EntryId, Step, Subagent, Subagents},
};

const DOT_SIZE: f32 = 7.;
/// Centres a dot on the first line of small text.
const DOT_TOP: f32 = 6.;

/// The card of the subagent the row's call started.
pub fn render(context: &EntryContext, cx: &App) -> AnyElement {
    let Some(agent) = context.agents.get(context.id) else {
        return div().into_any_element();
    };
    timeline_row(dot(agent.end, cx), context, cx)
        .children(card(agent, context, cx))
        .into_any_element()
}

/// The card of a subagent among the steps of the one that started it.
fn inner_card(id: EntryId, context: &EntryContext, cx: &App) -> AnyElement {
    let Some(agent) = context.agents.get(id) else {
        return div().into_any_element();
    };
    h_flex()
        .items_start()
        .gap_2()
        .child(small_dot(agent.end, cx))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap_1()
                .children(card(agent, context, cx)),
        )
        .into_any_element()
}

fn card(agent: &Subagent, context: &EntryContext, cx: &App) -> Vec<AnyElement> {
    let id = agent.id;
    let expanded = context.toggled_cards.contains(&id);
    let status = full_status(agent, context.agents);
    let status_line = div()
        .id(SharedString::from(format!("subagent-status-{id}")))
        .test_support()
        .aria_label(SharedString::from(status.clone()))
        .child(sub_line(
            &status,
            agent.end == Some(SubagentEnd::Failed),
            cx,
        ));
    let identity = identity(agent).map(|identity| {
        div()
            .pl_4()
            .text_caption()
            .text_color(cx.theme().muted_foreground)
            .child(identity)
            .into_any_element()
    });
    let details = expanded.then(|| details(agent, context, cx).into_any_element());
    [
        header(agent, expanded, context, cx),
        status_line.into_any_element(),
    ]
    .into_iter()
    .chain(identity)
    .chain(details)
    .collect()
}

fn header(agent: &Subagent, expanded: bool, context: &EntryContext, cx: &App) -> AnyElement {
    let id = agent.id;
    let view = context.view.clone();
    let open = Button::new(SharedString::from(format!("subagent-open-{id}")))
        .ghost()
        .row_icon(Lucide::PanelRightOpen)
        .tooltip("Open in Agents")
        .on_click(move |_, window, cx| {
            // The header's own click expands the card.
            cx.stop_propagation();
            let _ = view.update(cx, |view, cx| view.open_agents(Some(id), window, cx));
        });
    h_flex()
        .id(SharedString::from(format!("subagent-{id}")))
        .test_support()
        .aria_label(SharedString::from(agent.description.clone()))
        .gap_2()
        .cursor_pointer()
        .on_click(toggle_card(id, context))
        .child(
            div()
                .flex_none()
                .font_weight(FontWeight::SEMIBOLD)
                .child("Agent"),
        )
        .child(div().min_w_0().truncate().child(agent.description.clone()))
        .child(
            Icon::new(if expanded {
                Lucide::ChevronDown
            } else {
                Lucide::ChevronRight
            })
            .in_row()
            .text_color(cx.theme().muted_foreground),
        )
        .child(div().flex_1())
        .child(open)
        .into_any_element()
}

pub fn small_dot(end: Option<SubagentEnd>, cx: &App) -> impl IntoElement + use<> {
    div()
        .flex_none()
        .mt(px(DOT_TOP))
        .size(px(DOT_SIZE))
        .rounded_full()
        .bg(dot(end, cx))
}

/// Orange while it runs, then the colour of how it ended.
pub fn dot(end: Option<SubagentEnd>, cx: &App) -> Hsla {
    let theme = cx.theme();
    match end {
        None => spinner::accent(),
        Some(SubagentEnd::Completed) => theme.success,
        Some(SubagentEnd::Failed) => theme.danger,
        Some(SubagentEnd::Stopped) => theme.muted_foreground,
    }
}

/// Its status, and how many subagents are below it.
pub fn full_status(agent: &Subagent, agents: &Subagents) -> String {
    let below = agents.below(agent.id);
    let running = below.iter().filter(|agent| agent.end.is_none()).count();
    std::iter::once(status(agent))
        .chain(below_label(below.len(), running))
        .collect::<Vec<_>>()
        .join(" · ")
}

/// `2 subagents, 1 running`; `None` without any.
pub fn below_label(total: usize, running: usize) -> Option<String> {
    let noun = if total == 1 { "subagent" } else { "subagents" };
    match (total, running) {
        (0, _) => None,
        (_, 0) => Some(format!("{total} {noun}")),
        _ => Some(format!("{total} {noun}, {running} running")),
    }
}

/// `Reading note.txt · 1 tool · 14.4k tokens · 1.2s`, or how it ended.
pub fn status(agent: &Subagent) -> String {
    let state = match agent.end {
        None => agent.activity.clone().unwrap_or_else(|| "Starting…".into()),
        Some(SubagentEnd::Completed) => "Done".into(),
        Some(SubagentEnd::Failed) => "Failed".into(),
        Some(SubagentEnd::Stopped) => "Stopped".into(),
    };
    std::iter::once(state)
        .chain(agent.usage.iter().flat_map(usage_parts))
        .collect::<Vec<_>>()
        .join(" · ")
}

fn usage_parts(usage: &SubagentUsage) -> [String; 3] {
    let calls = if usage.tool_uses == 1 {
        "tool"
    } else {
        "tools"
    };
    [
        format!("{} {calls}", usage.tool_uses),
        format!("{} tokens", tokens(usage.tokens)),
        duration(usage.duration),
    ]
}

/// The kind of agent and the model it runs on, when known.
pub fn identity(agent: &Subagent) -> Option<String> {
    let parts: Vec<&str> = [agent.kind.as_deref(), agent.model.as_deref()]
        .into_iter()
        .flatten()
        .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// Its steps under a line of their own, then its report.
fn details(agent: &Subagent, context: &EntryContext, cx: &App) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let inner = |id| inner_card(id, context, cx);
    let place = StepPlace {
        root: context.root,
        chat: context.view,
        inner: &inner,
    };
    let steps: Vec<_> = (agent.steps.iter().enumerate())
        .map(|(index, step)| {
            let id = SharedString::from(format!("subagent-step-{}-{index}", agent.id));
            step_row(step, id, &place, cx)
        })
        .collect();
    let report = agent
        .summary
        .clone()
        .filter(|_| agent.end == Some(SubagentEnd::Completed));
    v_flex()
        .id(("subagent-steps", agent.id))
        .test_support()
        .ml_1()
        .pl_3()
        .gap_1()
        .border_l_1()
        .border_color(theme.border)
        .text_sm()
        .when(agent.steps.is_empty(), |this| {
            this.child(
                div()
                    .text_color(theme.muted_foreground)
                    .child("No steps yet"),
            )
        })
        .children(steps)
        .children(report.map(|report| {
            TextView::markdown(("subagent-report", agent.id), report).selectable(true)
        }))
}

/// What a step needs of where it shows.
pub struct StepPlace<'a> {
    pub root: &'a Path,
    pub chat: &'a WeakEntity<ChatView>,
    /// Draws a subagent the step started.
    pub inner: &'a dyn Fn(EntryId) -> AnyElement,
}

/// One thing the subagent did: its text, a tool call and its outcome, a
/// message it was sent or a subagent it started.
pub fn step_row(step: &Step, id: SharedString, place: &StepPlace, cx: &App) -> AnyElement {
    let theme = cx.theme();
    match step {
        Step::Text(text) => div()
            .text_color(theme.muted_foreground)
            .child(text.clone())
            .into_any_element(),
        Step::Message(message) => message_step(message, id, cx),
        Step::Subagent(inner) => (place.inner)(*inner),
        Step::Tool { call, result } => {
            let summary = result.as_ref().map(|result| {
                sub_line(
                    &tool_format::summary(&call.name, &result.output),
                    result.is_error,
                    cx,
                )
            });
            let pictures = result
                .as_ref()
                .filter(|result| !result.pictures.is_empty())
                .map(|result| {
                    pictures::gallery(format!("{id}-picture"), &result.pictures, place.chat, cx)
                });
            v_flex()
                .id(id)
                .test_support()
                .aria_label(SharedString::from(call.name.clone()))
                .child(
                    h_flex()
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
                                .font_family(theme.mono_font_family.clone())
                                .child(tool_format::detail(&call.input, place.root)),
                        ),
                )
                .children(summary)
                .children(pictures)
                .into_any_element()
        }
    }
}

/// `1.9s` under a minute, `2m 5s` above.
fn duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds < 60 {
        format!("{:.1}s", duration.as_secs_f32())
    } else {
        format!("{}m {}s", seconds / 60, seconds % 60)
    }
}

/// A message the subagent was sent after it started.
fn message_step(message: &str, id: SharedString, cx: &App) -> AnyElement {
    let theme = cx.theme();
    v_flex()
        .id(id)
        .test_support()
        .aria_label(SharedString::from(message.to_owned()))
        .gap_0p5()
        .px_2()
        .py_1()
        .rounded(theme.radius)
        .border_1()
        .border_color(theme.border)
        .bg(theme.secondary)
        .child(
            div()
                .text_caption()
                .text_color(theme.muted_foreground)
                .child("Message from Claude"),
        )
        .child(message.to_owned())
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_card_says_how_many_subagents_are_below_it() {
        assert_eq!(below_label(0, 0), None);
        assert_eq!(below_label(1, 0).as_deref(), Some("1 subagent"));
        assert_eq!(below_label(3, 2).as_deref(), Some("3 subagents, 2 running"));
    }

    #[test]
    fn durations_read_in_seconds_then_minutes() {
        assert_eq!(duration(Duration::from_millis(1_894)), "1.9s");
        assert_eq!(duration(Duration::from_secs(125)), "2m 5s");
    }
}
