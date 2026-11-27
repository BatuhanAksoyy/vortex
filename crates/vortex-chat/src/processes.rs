//! What the agents' processes use. The workspace reads the process table
//! and fills one shared [`Processes`]; each chat's header shows its own
//! agent's share and the sidebar shows them all. A click on either opens a
//! page listing the processes behind the numbers.

mod cells;
mod detail;
mod dialog;
mod rows;

pub use rows::{cpu as format_cpu, memory as format_memory};

use gpui_kit::prelude::*;
use gpui_kit::{
    App, Context, Entity, EventEmitter, Global, SharedString, Subscription, WeakEntity, Window,
    component::{
        ActiveTheme, Sizable as _,
        button::{Button, ButtonVariants as _},
    },
    div,
};
use vortex_services::ProcessInfo;

use crate::{ChatView, scale::ScaledText as _};
pub use dialog::{ProcessesDialog, ProcessesDialogEvent};
use rows::{cpu, memory};

/// A chat's agent process, with the processes under it after it.
#[derive(Clone)]
pub struct AgentProcesses {
    pub title: SharedString,
    pub processes: Vec<ProcessInfo>,
}

impl AgentProcesses {
    /// The agent's own process.
    fn pid(&self) -> Option<u32> {
        self.processes.first().map(|process| process.pid)
    }
}

/// The last reading of every agent's processes; `None` before the first.
#[derive(Default)]
pub struct Processes {
    agents: Option<Vec<AgentProcesses>>,
    /// Vortex's own process, apart from the agents it runs.
    own: Option<ProcessInfo>,
}

struct SharedProcesses(Entity<Processes>);

impl Global for SharedProcesses {}

impl Processes {
    /// The readings every chat and the sidebar show, made once per app.
    pub fn shared(cx: &mut App) -> Entity<Self> {
        if let Some(shared) = cx.try_global::<SharedProcesses>() {
            return shared.0.clone();
        }
        let processes = cx.new(|_| Self::default());
        cx.set_global(SharedProcesses(processes.clone()));
        processes
    }

    fn installed(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<SharedProcesses>()
            .map(|shared| shared.0.clone())
    }

    pub fn show(
        &mut self,
        agents: Vec<AgentProcesses>,
        own: Option<ProcessInfo>,
        cx: &mut Context<Self>,
    ) {
        let unchanged = (self.agents.as_ref()).is_some_and(|shown| looks_alike(shown, &agents))
            && own_usage(self.own.as_ref()) == own_usage(own.as_ref());
        self.agents = Some(agents);
        self.own = own;
        // A repaint redraws the whole window, long chats too, every couple
        // of seconds while nothing runs: only a reading that shows differs.
        if !unchanged {
            cx.notify();
        }
    }
}

/// What Vortex's own process uses, as shown.
fn own_usage(own: Option<&ProcessInfo>) -> Option<String> {
    own.map(|own| format!("{} CPU · {}", cpu(own.cpu), memory(own.memory)))
}

/// Whether `new` would show as `shown` does, numbers as they are rounded.
fn looks_alike(shown: &[AgentProcesses], new: &[AgentProcesses]) -> bool {
    shown.len() == new.len()
        && shown.iter().zip(new).all(|(shown, new)| {
            shown.title == new.title
                && shown.processes.len() == new.processes.len()
                && (shown.processes.iter().zip(&new.processes)).all(|(shown, new)| {
                    shown.pid == new.pid
                        && shown.command == new.command
                        && shown.status == new.status
                        && cpu(shown.cpu) == cpu(new.cpu)
                        && memory(shown.memory) == memory(new.memory)
                })
        })
}

/// Whose processes a meter or page shows.
#[derive(Clone)]
enum Scope {
    All,
    Chat(WeakEntity<ChatView>),
}

impl Scope {
    /// The agents in scope, or `None` before the first reading.
    fn agents<'a>(&self, processes: &'a Processes, cx: &App) -> Option<Vec<&'a AgentProcesses>> {
        let agents = processes.agents.as_ref()?;
        Some(match self {
            Self::All => agents.iter().collect(),
            Self::Chat(chat) => {
                let pid = chat.upgrade().and_then(|chat| chat.read(cx).process_id());
                agents
                    .iter()
                    .filter(|agent| pid.is_some() && agent.pid() == pid)
                    .collect()
            }
        })
    }
}

pub enum ProcessMeterEvent {
    /// Show the page listing the processes.
    Open,
}

/// CPU and memory of the processes in scope, as a button that opens the
/// page listing them.
pub struct ProcessMeter {
    processes: Entity<Processes>,
    scope: Scope,
    _observe: Subscription,
}

impl EventEmitter<ProcessMeterEvent> for ProcessMeter {}

impl ProcessMeter {
    /// Every agent's processes, for the sidebar.
    pub fn all(processes: Entity<Processes>, cx: &mut Context<Self>) -> Self {
        Self::new(processes, Scope::All, cx)
    }

    /// The processes of `chat`'s agent, if the app reads any.
    pub(crate) fn for_chat(chat: WeakEntity<ChatView>, cx: &mut App) -> Option<Entity<Self>> {
        let processes = Processes::installed(cx)?;
        Some(cx.new(|cx| Self::new(processes, Scope::Chat(chat), cx)))
    }

    fn new(processes: Entity<Processes>, scope: Scope, cx: &mut Context<Self>) -> Self {
        let observe = cx.observe(&processes, |_, _, cx| cx.notify());
        Self {
            processes,
            scope,
            _observe: observe,
        }
    }
}

impl Render for ProcessMeter {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let all = matches!(self.scope, Scope::All);
        let Some(agents) = self.scope.agents(self.processes.read(cx), cx) else {
            return div().into_any_element();
        };
        if agents.is_empty() && !all {
            return div().into_any_element();
        }
        let rows = rows::rows(&agents, "", rows::Order::default());
        let (cpu_total, memory_total) = rows::totals(&rows);
        let usage = format!("{} CPU · {}", cpu(cpu_total), memory(memory_total));
        let (id, label, tooltip) = if all {
            let own = own_usage(self.processes.read(cx).own.as_ref())
                .map_or_else(String::new, |own| format!("Vortex · {own}  ·  "));
            (
                "all-processes",
                format!("{own}Agents · {usage}"),
                "Vortex itself, and every agent's processes",
            )
        } else {
            ("chat-processes", usage, "This chat's processes")
        };
        Button::new(id)
            .ghost()
            .small()
            .px_1p5()
            .accessibility_label(label.clone())
            .child(div().text_caption().child(label))
            .text_color(cx.theme().muted_foreground)
            .tooltip(tooltip)
            .on_click(cx.listener(|_, _, _, cx| cx.emit(ProcessMeterEvent::Open)))
            .into_any_element()
    }
}
