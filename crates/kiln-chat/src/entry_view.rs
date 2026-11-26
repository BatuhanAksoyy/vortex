//! One transcript entry, laid out like the Claude Code VS Code panel: user
//! messages in a box, agent steps as a timeline of dots joined by a line.

mod thinking;

use std::{collections::HashSet, path::Path};

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Div, Entity, FontWeight, Hsla, SharedString, TestSupportExt, WeakEntity,
    component::{
        ActiveTheme, Icon, IconName, h_flex,
        text::{TextView, TextViewState},
        tooltip::Tooltip,
        v_flex,
    },
    div, px, relative,
};
use kiln_agent::{Message, ToolCall};

use crate::{
    chat_view::ChatView,
    code_block::{self, CodeLine, LineKind},
    literal::literal_markdown,
    pictures,
    scale::{ScaledIcon as _, ScaledText as _},
    spinner, subagent_view,
    tool_format::{self, EditDiff},
    transcript::{Entry, EntryId, Subagents, ToolResult},
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
    /// The cards toggled, also those inside this row's.
    pub toggled_cards: &'a HashSet<EntryId>,
    pub agents: &'a Subagents,
    pub view: &'a WeakEntity<ChatView>,
    pub root: &'a Path,
    /// How many times the transcript's width changed; see `width_watch`.
    pub width_changes: u32,
    /// Whether the timeline continues into the previous / next row.
    pub line_above: bool,
    pub line_below: bool,
    /// The text, with the search's matches, while a search shows them.
    pub found_text: Option<&'a Entity<TextViewState>>,
}

/// Rows that sit on the timeline; user messages and notices interrupt it.
pub fn on_timeline(entry: &Entry) -> bool {
    matches!(
        entry,
        Entry::Text { .. } | Entry::Thinking { .. } | Entry::Tool { .. } | Entry::Subagent
    )
}

pub fn render(entry: &Entry, context: &EntryContext, cx: &App) -> AnyElement {
    match entry {
        Entry::User(message) => user(message, context, cx),
        Entry::Notice { text, is_error } => notice(text, *is_error, context.id, cx),
        Entry::Text { text, .. } => {
            let body = context.found_text.map_or_else(
                || TextView::markdown(("text", context.id), text.clone()),
                TextView::new,
            );
            let body = body.selectable(true);
            timeline_row(cx.theme().muted_foreground, context, cx)
                .child(body)
                .into_any_element()
        }
        Entry::Thinking {
            text, tokens, took, ..
        } => thinking::render(text, *tokens, *took, context, cx),
        Entry::Tool { call, result } => tool(call, result.as_ref(), context, cx),
        Entry::Subagent => subagent_view::render(context, cx),
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

fn show_more(lines: usize, context: &EntryContext, cx: &App) -> impl IntoElement + use<> {
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
        .text_caption()
        .text_color(cx.theme().muted_foreground)
        .cursor_pointer()
        .child(label)
        .on_click(toggle(context))
}

/// A long message, such as a pasted document, shows its first lines until
/// expanded, so it cannot fill the panel.
const USER_PREVIEW_LINES: usize = 8;
/// The share of the transcript's width a message may take.
const USER_MAX_WIDTH: f32 = 0.85;

/// The markdown `entry` shows in full, which a search highlights its
/// matches in; `toggled` as in [`EntryContext`].
pub fn markdown_source(entry: &Entry, toggled: bool) -> Option<String> {
    match entry {
        Entry::Text { text, .. } => Some(text.clone()),
        Entry::User(message) => {
            let text = message.text.as_str();
            let whole = toggled || text.lines().count() <= USER_PREVIEW_LINES;
            (whole && !text.is_empty()).then(|| literal_markdown(text))
        }
        _ => None,
    }
}

fn user(message: &Message, context: &EntryContext, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let accent = spinner::accent();
    let text = message.text.as_str();
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
    let more = long.then(|| show_more(lines, context, cx));
    h_flex()
        .id(("user", context.id))
        .test_support()
        .aria_label(SharedString::from(text.to_owned()))
        .justify_end()
        .pb_3()
        .child(
            // On the right and tinted, so a message never reads as a
            // tool's output box.
            v_flex()
                .id(SharedString::from(format!("user-bubble-{}", context.id)))
                .test_support()
                .max_w(relative(USER_MAX_WIDTH))
                .min_w_0()
                .px_3()
                .py_1p5()
                .rounded(theme.radius * 1.5)
                .border_1()
                .border_color(accent.opacity(0.35))
                .bg(accent.opacity(0.1))
                .gap_1()
                .when(!message.pictures.is_empty(), |this| {
                    let id = format!("user-picture-{}", context.id);
                    this.child(pictures::gallery(id, &message.pictures, context.view, cx))
                })
                .when(!text.is_empty(), |this| {
                    let body = context.found_text.map_or_else(
                        || {
                            let id = format!("user-text-{}-{}", context.id, context.width_changes);
                            TextView::markdown(SharedString::from(id), literal_markdown(&shown))
                        },
                        TextView::new,
                    );
                    this.child(body.selectable(true))
                })
                .children(more)
                .children(message.files.iter().map(|path| attached_file(path, cx))),
        )
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
                .child(tool_format::title(&call.name)),
        )
        .child(
            div()
                .id(("tool-file", context.id))
                .test_support()
                .min_w_0()
                .truncate()
                .font_family(theme.mono_font_family.clone())
                .text_sm()
                .child(tool_format::detail(&call.input, context.root))
                .when_some(tool_format::file_path(&call.input), |this, path| {
                    this.hover(gpui_kit::StyleRefinement::underline)
                        .tooltip(|window, cx| Tooltip::new("Open in Files").build(window, cx))
                        .on_click(open_file(path.to_owned(), context))
                }),
        );
    let pictures = result
        .filter(|result| !result.pictures.is_empty())
        .map(|result| {
            pictures::gallery(
                format!("tool-picture-{}", context.id),
                &result.pictures,
                context.view,
                cx,
            )
        });
    timeline_row(dot, context, cx)
        .child(header)
        .children(tool_body(call, result, context, cx))
        .children(pictures)
        .into_any_element()
}

fn tool_body(
    call: &ToolCall,
    result: Option<&ToolResult>,
    context: &EntryContext,
    cx: &App,
) -> Vec<AnyElement> {
    if let Some(diff) = tool_format::edit_diff(&call.name, &call.input) {
        let error = result.filter(|result| result.is_error);
        let summary = error.map_or_else(|| diff.summary(), |result| result.output.clone());
        // Once the tool ran, its patch shows where in the file the lines
        // changed, around the lines left as they were.
        let patch = result.map_or(&[][..], |result| result.patch.as_slice());
        let (lines, numbered) = if patch.is_empty() {
            (diff_lines(&diff), diff.whole_file)
        } else {
            (code_block::patch_lines(patch), false)
        };
        return vec![
            sub_line(&summary, error.is_some(), cx),
            code_block::render(context.id, &lines, numbered, cx),
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
    vec![code_block::render(context.id, &lines, false, cx)]
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

/// A file sent with a message, named for the agent to read.
fn attached_file(path: &Path, cx: &App) -> impl IntoElement + use<> {
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    h_flex()
        .gap_1()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(Icon::new(IconName::File).in_row())
        .child(name)
}

/// The `└ Read 256 lines` line under a tool header.
pub fn sub_line(text: &str, is_error: bool, cx: &App) -> AnyElement {
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

fn open_file(
    path: String,
    context: &EntryContext,
) -> impl Fn(&gpui_kit::ClickEvent, &mut gpui_kit::Window, &mut App) + 'static {
    let view = context.view.clone();
    move |_, _, cx| {
        // The row's own click expands it; this one only opens the file.
        cx.stop_propagation();
        let _ = view.update(cx, |view, cx| view.open_file(&path, cx));
    }
}

pub fn toggle(
    context: &EntryContext,
) -> impl Fn(&gpui_kit::ClickEvent, &mut gpui_kit::Window, &mut App) + 'static {
    toggle_card(context.id, context)
}

pub fn toggle_card(
    entry: EntryId,
    context: &EntryContext,
) -> impl Fn(&gpui_kit::ClickEvent, &mut gpui_kit::Window, &mut App) + 'static {
    let view = context.view.clone();
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
