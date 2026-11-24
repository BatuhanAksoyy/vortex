//! A subagent's card on the timeline: what it was asked, how far it got and
//! what it used, and, expanded, its own steps and report.

use std::{path::Path, time::Duration};

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, FontWeight, Hsla, SharedString, TestSupportExt,
    component::{ActiveTheme, h_flex, text::TextView, v_flex},
    div,
};
use kiln_agent::{SubagentEnd, SubagentUsage};

use crate::{
    agent_state::tokens,
    entry_view::{EntryContext, sub_line, timeline_row, toggle},
    spinner, tool_format,
    transcript::{Step, Subagent},
};

pub fn render(agent: &Subagent, context: &EntryContext, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let dot = dot(agent.end, cx);
    let expanded = context.toggled;
    let header = h_flex()
        .id(SharedString::from(format!("subagent-{}", context.id)))
        .test_support()
        .aria_label(SharedString::from(agent.description.clone()))
        .gap_2()
        .cursor_pointer()
        .on_click(toggle(context))
        .child(
            div()
                .flex_none()
                .font_weight(FontWeight::SEMIBOLD)
                .child("Agent"),
        )
        .child(div().min_w_0().truncate().child(agent.description.clone()))
        .child(
            div()
                .flex_none()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(if expanded { "▾" } else { "▸" }),
        );
    let status = status(agent);
    timeline_row(dot, context, cx)
        .child(header)
        .child(
            div()
                .id(SharedString::from(format!(
                    "subagent-status-{}",
                    context.id
                )))
                .test_support()
                .aria_label(SharedString::from(status.clone()))
                .child(sub_line(
                    &status,
                    agent.end == Some(SubagentEnd::Failed),
                    cx,
                )),
        )
        .children(identity(agent).map(|identity| {
            div()
                .pl_4()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child(identity)
        }))
        .when(expanded, |this| this.child(details(agent, context, cx)))
        .into_any_element()
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
    let steps = agent.steps.iter().enumerate().map(|(index, step)| {
        let id = SharedString::from(format!("subagent-step-{}-{index}", context.id));
        step_row(step, id, context.root, cx)
    });
    let report = agent
        .summary
        .clone()
        .filter(|_| agent.end == Some(SubagentEnd::Completed));
    v_flex()
        .id(("subagent-steps", context.id))
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
            TextView::markdown(("subagent-report", context.id), report).selectable(true)
        }))
}

/// One thing the subagent did: its text, or a tool call and its outcome.
pub fn step_row(step: &Step, id: SharedString, root: &Path, cx: &App) -> AnyElement {
    let theme = cx.theme();
    match step {
        Step::Text(text) => div()
            .text_color(theme.muted_foreground)
            .child(text.clone())
            .into_any_element(),
        Step::Tool { call, result } => {
            let summary = result.as_ref().map(|result| {
                sub_line(
                    &tool_format::summary(&call.name, &result.output),
                    result.is_error,
                    cx,
                )
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
                                .child(call.name.clone()),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .font_family(theme.mono_font_family.clone())
                                .child(tool_format::detail(&call.input, root)),
                        ),
                )
                .children(summary)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_read_in_seconds_then_minutes() {
        assert_eq!(duration(Duration::from_millis(1_894)), "1.9s");
        assert_eq!(duration(Duration::from_secs(125)), "2m 5s");
    }
}
