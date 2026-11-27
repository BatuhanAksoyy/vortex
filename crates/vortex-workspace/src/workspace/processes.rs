//! Reading what each chat's agent runs, for the chats and the sidebar to
//! show, and the page of every agent's processes over the whole window.

use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, AsyncApp, Context, SharedString, Task, TestSupportExt, WeakEntity, Window,
    anchored,
    component::{ActiveTheme, TITLE_BAR_HEIGHT},
    deferred, div, point, px,
};
use vortex_services::Monitor;

use super::Workspace;
use vortex_chat::{AgentProcesses, ProcessesDialog, ProcessesDialogEvent};

/// How often the processes are read again. CPU use is measured over this
/// time; reading the whole process table costs a few milliseconds.
const REFRESH: Duration = Duration::from_secs(2);

impl Workspace {
    pub(super) fn open_processes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let page = cx.new(|cx| ProcessesDialog::all(window, cx));
        let closed = cx.subscribe_in(&page, window, |this, _, event, window, cx| match event {
            ProcessesDialogEvent::Close => this.close_processes(window, cx),
        });
        ProcessesDialog::focus(&page, window, cx);
        self.processes_page = Some((page, closed));
        cx.notify();
    }

    fn close_processes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.processes_page.take().is_some() {
            window.focus(&self.focus_handle, cx);
            cx.notify();
        }
    }

    /// The page over everything in the window under the title bar, chats
    /// included; the title bar stays clear for the window's own buttons.
    pub(super) fn render_processes(&self, window: &Window, cx: &App) -> Option<AnyElement> {
        let (page, _) = self.processes_page.as_ref()?;
        let size = window.viewport_size();
        let overlay = div()
            .id("all-processes-overlay")
            .test_support()
            .occlude()
            .w(size.width)
            .h(size.height - TITLE_BAR_HEIGHT)
            .p_4()
            .bg(cx.theme().overlay)
            .child(page.clone());
        Some(
            deferred(
                anchored()
                    .position(point(px(0.), TITLE_BAR_HEIGHT))
                    .child(overlay),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }

    pub(super) fn watch_processes(cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            let mut monitor = Monitor::new();
            while let Some(current) = monitor {
                monitor = tick(&this, current, cx).await;
                cx.background_executor().timer(REFRESH).await;
            }
        })
    }

    fn show_processes(&self, agents: Vec<AgentProcesses>, cx: &mut Context<Self>) {
        self.processes
            .update(cx, |processes, cx| processes.show(agents, cx));
    }
}

/// One reading, shown; `None` once the workspace is gone. The
/// process table is read on a background thread since that takes a while.
async fn tick(
    this: &WeakEntity<Workspace>,
    mut monitor: Monitor,
    cx: &mut AsyncApp,
) -> Option<Monitor> {
    let roots = this.update(cx, |this, cx| this.agent_roots(cx)).ok()?;
    let (monitor, agents) = cx
        .background_spawn(async move {
            // With no agent running there is nothing to read the table for.
            if !roots.is_empty() {
                monitor.refresh();
            }
            let agents = read(&monitor, roots);
            (monitor, agents)
        })
        .await;
    this.update(cx, |this, cx| this.show_processes(agents, cx))
        .ok()?;
    Some(monitor)
}

/// The trees under each running agent, leaving out one that has ended.
fn read(monitor: &Monitor, roots: Vec<(SharedString, u32)>) -> Vec<AgentProcesses> {
    roots
        .into_iter()
        .map(|(title, pid)| AgentProcesses {
            title,
            processes: monitor.tree(pid),
        })
        .filter(|agent| !agent.processes.is_empty())
        .collect()
}
