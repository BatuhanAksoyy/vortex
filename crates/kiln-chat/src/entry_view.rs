//! Presentation of one transcript entry, styled after the Claude Code CLI:
//! flat rows, tool calls as `Name(argument)` with indented output.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, SharedString, WeakEntity,
    component::{
        ActiveTheme, Icon, IconName, Sizable,
        button::{Button, ButtonVariants},
        h_flex,
        text::TextView,
        v_flex,
    },
    div,
};
use kiln_agent::Decision;

use crate::{
    chat_view::ChatView,
    transcript::{Entry, EntryId, ToolResult, tool_summary},
};

/// Tool output lines shown before the card is expanded.
const COLLAPSED_OUTPUT_LINES: usize = 6;

pub struct EntryContext {
    pub id: EntryId,
    pub expanded: bool,
    pub view: WeakEntity<ChatView>,
}

pub fn render(entry: &Entry, context: EntryContext, cx: &App) -> AnyElement {
    match entry {
        Entry::User(text) => user(text, cx),
        Entry::Text { text, .. } => TextView::markdown(("text", context.id), text.clone())
            .selectable(true)
            .into_any_element(),
        Entry::Thinking { text, .. } => div()
            .italic()
            .text_color(cx.theme().muted_foreground)
            .child(format!("∴ {text}"))
            .into_any_element(),
        Entry::Tool { call, result } => tool(
            &tool_summary(&call.name, &call.input),
            result.as_ref(),
            context,
            cx,
        ),
        Entry::Permission { request, decision } => permission(
            &request.id,
            &tool_summary(&request.tool, &request.input),
            *decision,
            &context,
            cx,
        ),
        Entry::Notice { text, is_error } => notice(text, *is_error, cx),
    }
}

fn user(text: &str, cx: &App) -> AnyElement {
    h_flex()
        .items_start()
        .gap_2()
        .px_2()
        .py_1()
        .rounded(cx.theme().radius)
        .bg(cx.theme().secondary)
        .child(div().text_color(cx.theme().muted_foreground).child(">"))
        .child(div().flex_1().child(text.to_owned()))
        .into_any_element()
}

fn tool(summary: &str, result: Option<&ToolResult>, context: EntryContext, cx: &App) -> AnyElement {
    let (icon, color) = match result {
        None => (IconName::Loader, cx.theme().muted_foreground),
        Some(ToolResult {
            is_error: false, ..
        }) => (IconName::CircleCheck, cx.theme().success),
        Some(ToolResult { is_error: true, .. }) => (IconName::CircleX, cx.theme().danger),
    };
    let view = context.view;
    let id = context.id;
    let header = h_flex()
        .id(("tool", id))
        .gap_2()
        .cursor_pointer()
        .child(Icon::new(icon).small().text_color(color))
        .child(
            div()
                .font_family(cx.theme().mono_font_family.clone())
                .truncate()
                .child(summary.to_owned()),
        )
        .on_click(move |_, _, cx| {
            let _ = view.update(cx, |view, cx| view.toggle_expanded(id, cx));
        });
    v_flex()
        .gap_1()
        .child(header)
        .when_some(result, |this, result| {
            this.child(output(&result.output, context.expanded, cx))
        })
        .into_any_element()
}

fn output(text: &str, expanded: bool, cx: &App) -> AnyElement {
    let lines: Vec<&str> = text.lines().collect();
    let shown = if expanded {
        lines.len()
    } else {
        lines.len().min(COLLAPSED_OUTPUT_LINES)
    };
    let hidden = lines.len() - shown;
    v_flex()
        .pl_6()
        .text_sm()
        .font_family(cx.theme().mono_font_family.clone())
        .text_color(cx.theme().muted_foreground)
        .children(
            lines[..shown]
                .iter()
                .map(|line| div().child((*line).to_owned())),
        )
        .when(hidden > 0, |this| {
            this.child(div().italic().child(format!("… {hidden} more lines")))
        })
        .into_any_element()
}

fn permission(
    request_id: &str,
    summary: &str,
    decision: Option<Decision>,
    context: &EntryContext,
    cx: &App,
) -> AnyElement {
    let body = v_flex()
        .gap_2()
        .p_3()
        .rounded(cx.theme().radius)
        .border_1()
        .border_color(cx.theme().warning)
        .child(div().child("Allow this tool call?"))
        .child(
            div()
                .font_family(cx.theme().mono_font_family.clone())
                .child(summary.to_owned()),
        );
    let footer: AnyElement = match decision {
        Some(Decision::Allow) => status_label("Allowed", cx.theme().success),
        Some(Decision::Deny) => status_label("Denied", cx.theme().danger),
        None => decision_buttons(request_id, context.id, &context.view),
    };
    body.child(footer).into_any_element()
}

fn decision_buttons(request_id: &str, id: EntryId, view: &WeakEntity<ChatView>) -> AnyElement {
    let button = |label: &'static str, decision: Decision| {
        let view = view.clone();
        let request_id = request_id.to_owned();
        Button::new((label, id))
            .label(label)
            .small()
            .on_click(move |_, _, cx| {
                let _ = view.update(cx, |view, cx| view.decide(&request_id, decision, cx));
            })
    };
    h_flex()
        .gap_2()
        .child(button("Allow", Decision::Allow).primary())
        .child(button("Deny", Decision::Deny).ghost())
        .into_any_element()
}

fn status_label(text: &'static str, color: gpui_kit::Hsla) -> AnyElement {
    div()
        .text_sm()
        .text_color(color)
        .child(SharedString::from(text))
        .into_any_element()
}

fn notice(text: &str, is_error: bool, cx: &App) -> AnyElement {
    let color = if is_error {
        cx.theme().danger
    } else {
        cx.theme().muted_foreground
    };
    div()
        .text_sm()
        .text_color(color)
        .child(text.to_owned())
        .into_any_element()
}
