//! The picked process beside the table: everything known about it, its
//! whole command line, and stopping it.

use std::{path::Path, time::SystemTime};

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Context, FontWeight, SharedString, TestSupportExt,
    component::{
        ActiveTheme, IconName, Sizable as _,
        button::{Button, ButtonVariants as _},
        h_flex, v_flex,
    },
    div, rems,
};

use super::{
    ProcessesDialog,
    cells::stop_button,
    rows::{Row, cpu, duration, memory},
};

const DETAIL_REMS: f32 = 22.0;
const LABEL_REMS: f32 = 7.0;

/// The details of process `pid`, given its row while it still runs.
pub(super) fn render(
    pid: u32,
    row: Option<Row>,
    all: bool,
    cx: &Context<ProcessesDialog>,
) -> AnyElement {
    let theme = cx.theme();
    let pane = v_flex()
        .id("process-detail")
        .test_support()
        .flex_none()
        .w(rems(DETAIL_REMS))
        .h_full()
        .overflow_y_scroll()
        .p_3()
        .gap_3()
        .border_l_1()
        .border_color(theme.border);
    let close = close_button(cx);
    let Some(row) = row else {
        let text = format!("Process {pid} has ended.");
        return pane
            .aria_label(SharedString::from(text.clone()))
            .child(
                h_flex()
                    .child(
                        div()
                            .flex_1()
                            .text_color(theme.muted_foreground)
                            .child(text),
                    )
                    .child(close),
            )
            .into_any_element();
    };
    let process = row.process;
    let name = program(&process.command);
    let action = if row.is_agent() {
        div()
            .text_xs()
            .text_color(theme.muted_foreground)
            .child("This is the chat's agent. Close its tab to stop it.")
            .into_any_element()
    } else {
        stop_button("process-detail-stop", pid).into_any_element()
    };
    pane.aria_label(SharedString::from(name.clone()))
        .child(
            h_flex()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .font_weight(FontWeight::SEMIBOLD)
                        .truncate()
                        .child(name),
                )
                .child(close),
        )
        .child(facts(row, all, cx))
        .child(command(&process.command, cx))
        .child(h_flex().child(action))
        .into_any_element()
}

fn close_button(cx: &Context<ProcessesDialog>) -> Button {
    Button::new("process-detail-close")
        .ghost()
        .xsmall()
        .icon(IconName::Close)
        .tooltip("Close details")
        .on_click(cx.listener(|page, _, _, cx| page.close_details(cx)))
}

fn facts(row: Row, all: bool, cx: &App) -> impl IntoElement {
    let process = row.process;
    let now = SystemTime::now();
    let running = now
        .duration_since(process.started_at)
        .map_or_else(|_| "—".to_owned(), duration);
    let path =
        |path: Option<&Path>| path.map_or_else(|| "—".to_owned(), |p| p.display().to_string());
    v_flex()
        .gap_1()
        .when(all, |this| {
            this.child(fact("Chat", row.chat.to_string(), cx))
        })
        .child(fact("PID", process.pid.to_string(), cx))
        .child(fact(
            "Parent",
            process
                .parent
                .map_or_else(|| "—".to_owned(), |pid| pid.to_string()),
            cx,
        ))
        .child(fact("Status", process.status.clone(), cx))
        .child(fact("Started", started(process.started_at), cx))
        .child(fact("Running for", running, cx))
        .child(fact("CPU", cpu(process.cpu), cx))
        .child(fact("Memory", memory(process.memory), cx))
        .child(fact("Directory", path(process.cwd.as_deref()), cx))
        .child(fact("Executable", path(process.exe.as_deref()), cx))
}

fn fact(name: &'static str, value: String, cx: &App) -> impl IntoElement {
    h_flex()
        .id(SharedString::from(format!(
            "process-fact-{}",
            name.to_lowercase().replace(' ', "-")
        )))
        .test_support()
        .aria_label(SharedString::from(value.clone()))
        .items_start()
        .gap_2()
        .child(
            div()
                .flex_none()
                .w(rems(LABEL_REMS))
                .text_color(cx.theme().muted_foreground)
                .child(name),
        )
        .child(div().min_w_0().flex_1().overflow_hidden().child(value))
}

fn command(text: &str, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    v_flex()
        .gap_1()
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("Command line"),
        )
        .child(
            div()
                .id("process-command")
                .test_support()
                .aria_label(SharedString::from(text.to_owned()))
                .p_2()
                .rounded(theme.radius)
                .bg(theme.secondary)
                .overflow_hidden()
                .text_xs()
                .font_family(theme.mono_font_family.clone())
                .child(text.to_owned()),
        )
}

/// The program's name: the command's first word, without its directory.
fn program(command: &str) -> String {
    let first = command.split_whitespace().next().unwrap_or(command);
    Path::new(first).file_name().map_or_else(
        || first.to_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// "2026-11-24 14:05:09", local time.
fn started(at: SystemTime) -> String {
    jiff::Timestamp::try_from(at).map_or_else(
        |_| "—".to_owned(),
        |at| {
            at.to_zoned(jiff::tz::TimeZone::system())
                .strftime("%Y-%m-%d %H:%M:%S")
                .to_string()
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_is_named_without_its_directory() {
        assert_eq!(program("/usr/local/bin/node server.js --port 3000"), "node");
        assert_eq!(program("sleep 30"), "sleep");
        assert_eq!(program(""), "");
    }
}
