//! The processes page: every process in scope as a row with what it uses,
//! under its chat when it shows them all; a search over them, sorting by a
//! column's header either way, and the picked one's details beside the
//! table. It follows the readings live.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Context, Entity, EventEmitter, FocusHandle, FontWeight, KeyDownEvent,
    SharedString, Subscription, TestSupportExt, WeakEntity, Window,
    component::{
        ActiveTheme, IconName, Sizable as _,
        button::{Button, ButtonVariants as _},
        h_flex,
        input::{Input, InputEvent, InputState},
        v_flex,
    },
    div, rems,
};

use super::{
    Processes, Scope,
    cells::{Cells, STOP_REMS, chat_heading, columns, stop_button},
    detail,
    rows::{self, Order, Row, Sort, cpu, memory},
};
use crate::{
    ChatView,
    scale::{ScaledButton as _, ScaledText as _},
};

const INDENT_REMS: f32 = 1.0;
const SEARCH_REMS: f32 = 16.0;

pub enum ProcessesDialogEvent {
    Close,
}

pub struct ProcessesDialog {
    processes: Entity<Processes>,
    scope: Scope,
    search: Entity<InputState>,
    order: Order,
    /// The process whose details show, by PID.
    selected: Option<u32>,
    focus: FocusHandle,
    _subscriptions: [Subscription; 2],
}

impl EventEmitter<ProcessesDialogEvent> for ProcessesDialog {}

impl ProcessesDialog {
    /// Every agent's processes.
    pub fn all(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let processes = super::Processes::shared(cx);
        Self::new(processes, Scope::All, window, cx)
    }

    /// The processes of `chat`'s agent, if the app reads any.
    pub(crate) fn for_chat(
        chat: WeakEntity<ChatView>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Entity<Self>> {
        let processes = Processes::installed(cx)?;
        Some(cx.new(|cx| Self::new(processes, Scope::Chat(chat), window, cx)))
    }

    fn new(
        processes: Entity<Processes>,
        scope: Scope,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search commands, PIDs, chats"));
        let subscriptions = [
            cx.observe(&processes, |_, _, cx| cx.notify()),
            cx.subscribe(&search, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
        ];
        Self {
            processes,
            scope,
            search,
            order: Order::default(),
            selected: None,
            focus: cx.focus_handle(),
            _subscriptions: subscriptions,
        }
    }

    /// Focuses the page, not its search, so typing starts only after a
    /// click in the box, and Escape works at once.
    pub fn focus(page: &Entity<Self>, window: &mut Window, cx: &mut App) {
        let focus = page.read(cx).focus.clone();
        window.focus(&focus, cx);
    }

    /// Escape closes the details first, then the page.
    fn on_key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key != "escape" {
            return;
        }
        if self.selected.take().is_some() {
            cx.notify();
        } else {
            cx.emit(ProcessesDialogEvent::Close);
        }
        cx.stop_propagation();
    }

    fn sort_by(&mut self, sort: Sort, cx: &mut Context<Self>) {
        self.order = self.order.then_by(sort);
        cx.notify();
    }

    /// Shows `pid`'s details, or hides them when they show already.
    fn select(&mut self, pid: u32, cx: &mut Context<Self>) {
        self.selected = (self.selected != Some(pid)).then_some(pid);
        cx.notify();
    }

    pub(super) fn close_details(&mut self, cx: &mut Context<Self>) {
        self.selected = None;
        cx.notify();
    }

    /// Whether the rows sit under their chat's heading, which they do in
    /// the tree order of every agent.
    fn grouped(&self) -> bool {
        matches!(self.scope, Scope::All) && self.order.sort == Sort::Tree
    }

    /// Whether the rows name their chat: sorted, they leave its heading.
    fn chat_column(&self) -> bool {
        matches!(self.scope, Scope::All) && !self.grouped()
    }

    fn title(&self, cx: &App) -> SharedString {
        match &self.scope {
            Scope::All => "All agents' processes".into(),
            Scope::Chat(chat) => {
                let title = chat
                    .upgrade()
                    .and_then(|chat| chat.read(cx).title().map(str::to_owned));
                format!("Processes · {}", title.as_deref().unwrap_or("New session")).into()
            }
        }
    }

    fn top_bar(&self, rows: &[Row], cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (cpu_total, memory_total) = rows::totals(rows);
        let count = if rows.len() == 1 {
            "process"
        } else {
            "processes"
        };
        let mut summary = format!(
            "{} {count} · {} CPU · {}",
            rows.len(),
            cpu(cpu_total),
            memory(memory_total)
        );
        let own = super::own_usage(self.processes.read(cx).own.as_ref());
        if let (Scope::All, Some(own)) = (&self.scope, own) {
            summary = format!("{summary}  ·  Vortex itself {own}");
        }
        h_flex()
            .gap_3()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(theme.border)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(self.title(cx)),
                    )
                    .child(
                        div()
                            .id("processes-summary")
                            .test_support()
                            .aria_label(SharedString::from(summary.clone()))
                            .text_caption()
                            .text_color(theme.muted_foreground)
                            .child(summary),
                    ),
            )
            .child(
                div()
                    .id("processes-search")
                    .test_support()
                    .w(rems(SEARCH_REMS))
                    .child(Input::new(&self.search).small().cleanable(true)),
            )
            .child(
                Button::new("processes-close")
                    .ghost()
                    .row_icon(IconName::Close)
                    .tooltip("Close")
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(ProcessesDialogEvent::Close))),
            )
    }

    /// A column's header; a click sorts by it.
    fn heading(
        &self,
        id: &'static str,
        label: &'static str,
        sort: Sort,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let active = self.order.sort == sort;
        let arrow = match (active, sort, self.order.descending) {
            (false, _, _) | (_, Sort::Tree, _) => "",
            (true, _, true) => " ↓",
            (true, _, false) => " ↑",
        };
        div()
            .id(id)
            .test_support()
            .cursor_pointer()
            .when(active, |this| this.text_color(cx.theme().foreground))
            .hover(|style| style.text_color(cx.theme().foreground))
            .on_click(cx.listener(move |this, _, _, cx| this.sort_by(sort, cx)))
            .child(format!("{label}{arrow}"))
    }

    fn header(&self, cx: &Context<Self>) -> impl IntoElement {
        columns(
            self.chat_column(),
            self.heading("processes-sort-tree", "Process", Sort::Tree, cx),
            Cells {
                chat: div().child("Chat"),
                pid: self.heading("processes-sort-pid", "PID", Sort::Pid, cx),
                cpu: self.heading("processes-sort-cpu", "CPU", Sort::Cpu, cx),
                memory: self.heading("processes-sort-memory", "Memory", Sort::Memory, cx),
            },
        )
        .px_3()
        .py_1p5()
        .text_color(cx.theme().muted_foreground)
        .border_b_1()
        .border_color(cx.theme().border)
        .child(div().w(rems(STOP_REMS)))
    }

    fn row(&self, row: Row, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let process = row.process;
        let pid = process.pid;
        // Sorted rows no longer sit under their parents.
        let depth = if self.order.sort == Sort::Tree {
            process.depth
        } else {
            0
        };
        let name = div()
            .truncate()
            .pl(rems(
                INDENT_REMS * f32::from(u16::try_from(depth).unwrap_or(u16::MAX)),
            ))
            .font_family(theme.mono_font_family.clone())
            .child(process.command.clone());
        let stop = (!row.is_agent()).then(|| stop_button(format!("process-stop-{pid}"), pid));
        columns(
            self.chat_column(),
            name,
            Cells {
                chat: div().truncate().child(row.chat.clone()),
                pid: div().child(pid.to_string()),
                cpu: div().child(cpu(process.cpu)),
                memory: div().child(memory(process.memory)),
            },
        )
        .id(SharedString::from(format!("process-{pid}")))
        .test_support()
        .aria_label(SharedString::from(process.command.clone()))
        .px_3()
        .py_1()
        .cursor_pointer()
        .when(self.selected == Some(pid), |this| this.bg(theme.secondary))
        .hover(|style| style.bg(theme.secondary))
        .on_click(cx.listener(move |this, _, _, cx| this.select(pid, cx)))
        .child(div().w(rems(STOP_REMS)).flex().justify_end().children(stop))
        .into_any_element()
    }

    /// The rows, under a heading per chat when grouped.
    fn body(&self, rows: &[Row], cx: &Context<Self>) -> Vec<AnyElement> {
        if !self.grouped() {
            return rows.iter().map(|row| self.row(*row, cx)).collect();
        }
        rows.chunk_by(|a, b| a.agent == b.agent)
            .flat_map(|group| {
                std::iter::once(chat_heading(group, cx))
                    .chain(group.iter().map(|row| self.row(*row, cx)))
            })
            .collect()
    }

    fn table(&self, rows: &[Row], cx: &Context<Self>) -> impl IntoElement {
        let query = self.search.read(cx).value();
        let empty = rows.is_empty().then(|| {
            let text = if query.trim().is_empty() {
                "No agent is running. An agent starts when its chat sends a message.".to_owned()
            } else {
                format!("No process matches “{}”.", query.trim())
            };
            div()
                .id("processes-empty")
                .test_support()
                .aria_label(SharedString::from(text.clone()))
                .p_3()
                .text_color(cx.theme().muted_foreground)
                .child(text)
        });
        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(self.header(cx))
            .child(
                v_flex()
                    .id("processes-rows")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .children(empty)
                    .children(self.body(rows, cx)),
            )
    }
}

impl Render for ProcessesDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let all = matches!(self.scope, Scope::All);
        let processes = self.processes.read(cx);
        let agents = self.scope.agents(processes, cx).unwrap_or_default();
        let query = self.search.read(cx).value();
        let rows = rows::rows(&agents, &query, self.order);
        // Picked from every process in scope, so a search does not hide it.
        let selected = self.selected.and_then(|pid| {
            rows::rows(&agents, "", Order::default())
                .into_iter()
                .find(|row| row.process.pid == pid)
        });
        let detail = self
            .selected
            .map(|pid| detail::render(pid, selected, all, cx));
        v_flex()
            .id("processes-dialog")
            .test_support()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .size_full()
            .rounded(theme.radius_lg)
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .shadow_lg()
            .text_sm()
            .child(self.top_bar(&rows, cx))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .items_start()
                    .child(self.table(&rows, cx))
                    .children(detail),
            )
    }
}
