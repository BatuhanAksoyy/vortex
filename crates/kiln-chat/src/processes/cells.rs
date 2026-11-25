//! The columns every row of a processes page lines up in, a chat's
//! heading over its rows, and the button that stops a process.

use super::rows::{self, Row, cpu, memory};

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, ClickEvent, FontWeight, SharedString, TestSupportExt,
    component::{
        ActiveTheme, Sizable as _,
        button::{Button, ButtonVariants as _},
        h_flex,
    },
    div, rems,
};

const CHAT_REMS: f32 = 10.0;
const PID_REMS: f32 = 4.5;
const CPU_REMS: f32 = 4.0;
const MEMORY_REMS: f32 = 5.5;
pub(super) const STOP_REMS: f32 = 3.5;

/// The cells after a row's name.
pub(super) struct Cells<A, B, C, D> {
    pub chat: A,
    pub pid: B,
    pub cpu: C,
    pub memory: D,
}

/// A row with the page's columns; the chat's only when `chat` is set.
pub(super) fn columns(
    chat: bool,
    name: impl IntoElement,
    cells: Cells<impl IntoElement, impl IntoElement, impl IntoElement, impl IntoElement>,
) -> gpui_kit::Div {
    let number =
        |width: f32, cell: AnyElement| div().w(rems(width)).flex().justify_end().child(cell);
    h_flex()
        .gap_3()
        .child(div().flex_1().min_w_0().child(name))
        .when(chat, |this| {
            this.child(div().w(rems(CHAT_REMS)).min_w_0().child(cells.chat))
        })
        .child(number(PID_REMS, cells.pid.into_any_element()))
        .child(number(CPU_REMS, cells.cpu.into_any_element()))
        .child(number(MEMORY_REMS, cells.memory.into_any_element()))
}

/// The heading of a chat's rows: its name and what they use together.
pub(super) fn chat_heading(rows: &[Row], cx: &App) -> AnyElement {
    let theme = cx.theme();
    let (cpu_total, memory_total) = rows::totals(rows);
    let first = rows.first().copied();
    let (agent, chat) = first.map_or((0, SharedString::default()), |row| {
        (row.agent, row.chat.clone())
    });
    columns(
        false,
        div().truncate().child(chat.clone()),
        Cells {
            chat: div(),
            pid: div(),
            cpu: div().child(cpu(cpu_total)),
            memory: div().child(memory(memory_total)),
        },
    )
    .id(SharedString::from(format!("processes-chat-{agent}")))
    .test_support()
    .aria_label(chat)
    .mt_2()
    .px_3()
    .py_1p5()
    .bg(theme.muted)
    .border_y_1()
    .border_color(theme.border)
    .font_weight(FontWeight::SEMIBOLD)
    .child(div().w(rems(STOP_REMS)))
    .into_any_element()
}

pub(super) fn stop_button(id: impl Into<SharedString>, pid: u32) -> Button {
    Button::new(id.into())
        .ghost()
        .small()
        .label("Stop")
        .tooltip("Stop it and everything it started")
        .on_click(move |_: &ClickEvent, _, cx| {
            // The row under the button would take the click too.
            cx.stop_propagation();
            cx.background_spawn(async move { stop(pid) }).detach();
        })
}

fn stop(pid: u32) {
    if !kiln_services::stop(pid) {
        log::warn!("cannot stop process {pid}");
    }
}
