//! One transcript entry, laid out like the Claude Code VS Code panel: user
//! messages in a box, agent steps as a timeline of dots joined by a line.

use std::path::Path;

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Div, FontWeight, Hsla, SharedString, TestSupportExt, WeakEntity,
    component::{ActiveTheme, h_flex, text::TextView, v_flex},
    div, px,
};
use kiln_agent::ToolCall;

use crate::{
    chat_view::ChatView,
    code_block::{self, CodeLine, LineKind},
    tool_format::{self, EditDiff},
    transcript::{Entry, EntryId, ToolResult},
};

/// Vertical centre of the first text line, where the dot sits.
const DOT_CENTER: f32 = 12.;
const DOT_SIZE: f32 = 7.;
/// Left edge of the timeline line, centred under the dot.
const LINE_LEFT: f32 = 3.;

pub struct EntryContext<'a> {
    pub id: EntryId,
    /// Whether the user toggled this row away from its default.
    pub toggled: bool,
    pub view: &'a WeakEntity<ChatView>,
    pub root: &'a Path,
    /// Whether the timeline continues into the previous / next row.
    pub line_above: bool,
    pub line_below: bool,
}

/// Rows that sit on the timeline; user messages and notices interrupt it.
pub fn on_timeline(entry: &Entry) -> bool {
    matches!(
        entry,
        Entry::Text { .. } | Entry::Thinking { .. } | Entry::Tool { .. }
    )
}

pub fn render(entry: &Entry, context: &EntryContext, cx: &App) -> AnyElement {
    match entry {
        Entry::User(text) => user(text, context, cx),
        Entry::Notice { text, is_error } => notice(text, *is_error, context.id, cx),
        Entry::Text { text, .. } => {
            let body = TextView::markdown(("text", context.id), text.clone()).selectable(true);
            timeline_row(cx.theme().muted_foreground, context, cx)
                .child(body)
                .into_any_element()
        }
        Entry::Thinking { text, .. } => thinking(text, context, cx),
        Entry::Tool { call, result } => tool(call, result.as_ref(), context, cx),
    }
}

/// A row with its dot and its share of the timeline line; the caller adds
/// the content.
pub fn timeline_row(dot: Hsla, context: &EntryContext, cx: &App) -> Div {
    let line = cx.theme().border;
    let segment = match (context.line_above, context.line_below) {
        (true, true) => Some(div().top_0().bottom_0()),
        (true, false) => Some(div().top_0().h(px(DOT_CENTER))),
        (false, true) => Some(div().top(px(DOT_CENTER)).bottom_0()),
        (false, false) => None,
    };
    v_flex()
        .relative()
        .pl_5()
        .pb_3()
        .gap_1()
        .when_some(segment, |this, segment| {
            this.child(segment.absolute().left(px(LINE_LEFT)).w(px(1.)).bg(line))
        })
        .child(
            div()
                .absolute()
                .left(px(LINE_LEFT + 0.5 - DOT_SIZE / 2.))
                .top(px(DOT_CENTER - DOT_SIZE / 2.))
                .size(px(DOT_SIZE))
                .rounded_full()
                .bg(dot),
        )
}

/// A long message, such as a pasted document, shows its first lines until
/// expanded, so it cannot fill the panel.
const USER_PREVIEW_LINES: usize = 8;

fn user(text: &str, context: &EntryContext, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let lines = text.lines().count();
    let long = lines > USER_PREVIEW_LINES;
    let shown = if long && !context.toggled {
        text.lines()
            .take(USER_PREVIEW_LINES)
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        text.to_owned()
    };
    let more = long.then(|| {
        let label = if context.toggled {
            "Show less".to_owned()
        } else {
            format!("Show all {lines} lines")
        };
        div()
            .id(("user-more", context.id))
            .test_support()
            .aria_label(SharedString::from(label.clone()))
            .pt_1()
            .text_xs()
            .text_color(theme.muted_foreground)
            .cursor_pointer()
            .child(label)
            .on_click(toggle(context))
    });
    h_flex()
        .id(("user", context.id))
        .test_support()
        .aria_label(SharedString::from(text.to_owned()))
        .pb_3()
        .child(
            v_flex()
                .max_w_full()
                .min_w_0()
                .px_3()
                .py_1p5()
                .rounded(theme.radius * 1.5)
                .border_1()
                .border_color(theme.border)
                .bg(theme.secondary)
                .child(shown)
                .children(more),
        )
        .into_any_element()
}

fn thinking(text: &str, context: &EntryContext, cx: &App) -> AnyElement {
    let expanded = context.toggled;
    let muted = cx.theme().muted_foreground;
    timeline_row(muted, context, cx)
        .child(
            div()
                .id(("thinking", context.id))
                .test_support()
                .cursor_pointer()
                .italic()
                .text_color(muted)
                .child(if expanded {
                    "Thinking ▾"
                } else {
                    "Thinking ▸"
                })
                .on_click(toggle(context)),
        )
        .when(expanded, |this| {
            this.child(div().italic().text_color(muted).child(text.to_owned()))
        })
        .into_any_element()
}

fn tool(
    call: &ToolCall,
    result: Option<&ToolResult>,
    context: &EntryContext,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let dot = match result {
        None => theme.muted_foreground,
        Some(result) if result.is_error => theme.danger,
        Some(_) => theme.success,
    };
    let header = h_flex()
        .id(("tool", context.id))
        .test_support()
        .gap_2()
        .cursor_pointer()
        .on_click(toggle(context))
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
                .text_sm()
                .child(tool_format::detail(&call.input, context.root)),
        );
    timeline_row(dot, context, cx)
        .child(header)
        .children(tool_body(call, result, context, cx))
        .into_any_element()
}

fn tool_body(
    call: &ToolCall,
    result: Option<&ToolResult>,
    context: &EntryContext,
    cx: &App,
) -> Vec<AnyElement> {
    let id = ("tool-output", context.id);
    if let Some(diff) = tool_format::edit_diff(&call.name, &call.input) {
        let error = result.filter(|result| result.is_error);
        let summary = error.map_or_else(|| diff.summary(), |result| result.output.clone());
        return vec![
            sub_line(&summary, error.is_some(), cx),
            code_block::render(id, &diff_lines(&diff), diff.whole_file, cx),
        ];
    }
    let Some(result) = result else {
        return Vec::new();
    };
    let full = tool_format::shows_output(&call.name) != context.toggled;
    if !full || result.output.trim().is_empty() {
        return vec![sub_line(
            &tool_format::summary(&call.name, &result.output),
            result.is_error,
            cx,
        )];
    }
    let lines: Vec<CodeLine> = result
        .output
        .lines()
        .map(|line| CodeLine::new(line, LineKind::Plain))
        .collect();
    vec![code_block::render(id, &lines, false, cx)]
}

fn diff_lines(diff: &EditDiff) -> Vec<CodeLine> {
    let removed = diff
        .removed
        .iter()
        .map(|line| CodeLine::new(line.as_str(), LineKind::Removed));
    let added = diff
        .added
        .iter()
        .map(|line| CodeLine::new(line.as_str(), LineKind::Added));
    removed.chain(added).collect()
}

/// The `└ Read 256 lines` line under a tool header.
fn sub_line(text: &str, is_error: bool, cx: &App) -> AnyElement {
    let color = if is_error {
        cx.theme().danger
    } else {
        cx.theme().muted_foreground
    };
    let first = text.lines().next().unwrap_or_default();
    div()
        .pl_1()
        .text_sm()
        .truncate()
        .text_color(color)
        .child(format!("└ {first}"))
        .into_any_element()
}

fn toggle(
    context: &EntryContext,
) -> impl Fn(&gpui_kit::ClickEvent, &mut gpui_kit::Window, &mut App) + 'static {
    let view = context.view.clone();
    let entry = context.id;
    move |_, _, cx| {
        let _ = view.update(cx, |view, cx| view.toggle_expanded(entry, cx));
    }
}

fn notice(text: &str, is_error: bool, id: u64, cx: &App) -> AnyElement {
    let color = if is_error {
        cx.theme().danger
    } else {
        cx.theme().muted_foreground
    };
    div()
        .id(("notice", id))
        .test_support()
        .aria_label(SharedString::from(text.to_owned()))
        .pb_3()
        .text_sm()
        .text_color(color)
        .child(SharedString::from(text.to_owned()))
        .into_any_element()
}
