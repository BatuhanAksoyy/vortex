//! What the page and the browser cost: the page's script memory and
//! document size, and the processor and memory of the browser's processes.

use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, AsyncApp, Context, FontWeight, TestSupportExt, WeakEntity,
    component::{ActiveTheme, Theme, h_flex, v_flex},
    div,
};
use vortex_browser::{Page, PageMetrics};
use vortex_chat::{format_cpu, format_memory};
use vortex_services::Monitor;

use super::{Browser, tools_panel::Tool};

/// CPU use is measured over this time.
const REFRESH: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct BrowserUse {
    cpu: f32,
    memory: u64,
    processes: usize,
}

#[derive(Default)]
pub(super) struct Usage {
    page: Option<PageMetrics>,
    browser: Option<BrowserUse>,
    watching: Option<gpui_kit::Task<()>>,
}

impl Browser {
    /// Reads the use again and again, while it is in sight.
    pub(super) fn watch_usage(&mut self, cx: &mut Context<Self>) {
        if self.usage.watching.is_some() {
            return;
        }
        self.usage.watching = Some(cx.spawn(async move |this, cx| {
            let mut monitor = Monitor::new();
            while let Some(kept) = usage_tick(&this, monitor, cx).await {
                monitor = kept;
                cx.background_executor().timer(REFRESH).await;
            }
            let _ = this.update(cx, |this, _| this.usage.watching = None);
        }));
    }

    /// The page and the browser to measure, while the use is in sight.
    fn usage_target(&self) -> Option<(Option<Page>, Option<u32>)> {
        let shown = self.showing_tool(Tool::Usage);
        shown.then(|| (self.page().cloned(), self.chrome_pid()))
    }

    fn show_usage(
        &mut self,
        page: Option<PageMetrics>,
        browser: Option<BrowserUse>,
        cx: &mut Context<Self>,
    ) {
        self.usage.page = page;
        self.usage.browser = browser;
        cx.notify();
    }

    pub(super) fn render_usage(&self, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let page = self.usage.page.map(|page| {
            vec![
                (
                    "Script memory",
                    format!(
                        "{} of {}",
                        format_memory(page.heap_used),
                        format_memory(page.heap_total)
                    ),
                ),
                ("Elements", page.nodes.to_string()),
                ("Event listeners", page.listeners.to_string()),
                ("Documents", page.documents.to_string()),
            ]
        });
        let browser = self.usage.browser.map(|browser| {
            vec![
                ("CPU", format_cpu(browser.cpu)),
                ("Memory", format_memory(browser.memory)),
                ("Processes", browser.processes.to_string()),
            ]
        });
        h_flex()
            .id("browser-usage")
            .test_support()
            .size_full()
            .items_start()
            .gap_6()
            .p_3()
            .text_sm()
            .child(group("This Page", page, theme))
            .child(group("Browser, Every Page", browser, theme))
            .into_any_element()
    }
}

/// One reading, shown; `None` once it is out of sight. The process table
/// is read on a background thread since that takes a while.
async fn usage_tick(
    this: &WeakEntity<Browser>,
    monitor: Option<Monitor>,
    cx: &mut AsyncApp,
) -> Option<Option<Monitor>> {
    let (page, pid) = this.update(cx, |this, _| this.usage_target()).ok()??;
    let (monitor, browser) = cx
        .background_spawn(async move { browser_use(monitor, pid) })
        .await;
    let metrics = read_metrics(page).await;
    this.update(cx, |this, cx| this.show_usage(metrics, browser, cx))
        .ok()?;
    Some(monitor)
}

/// The browser's processes, read again; `None` where there is no process
/// table or the browser is gone.
fn browser_use(
    mut monitor: Option<Monitor>,
    pid: Option<u32>,
) -> (Option<Monitor>, Option<BrowserUse>) {
    let (Some(reading), Some(pid)) = (monitor.as_mut(), pid) else {
        return (monitor, None);
    };
    reading.refresh();
    let tree = reading.tree(pid);
    let read = (!tree.is_empty()).then(|| BrowserUse {
        cpu: tree.iter().map(|process| process.cpu).sum(),
        memory: tree.iter().map(|process| process.memory).sum(),
        processes: tree.len(),
    });
    (monitor, read)
}

async fn read_metrics(page: Option<Page>) -> Option<PageMetrics> {
    page?.metrics().await.ok()
}

fn group(
    title: &str,
    rows: Option<Vec<(&str, String)>>,
    theme: &Theme,
) -> impl IntoElement + use<> {
    let rows = rows.map_or_else(
        || {
            vec![
                div()
                    .text_color(theme.muted_foreground)
                    .child("Measuring…")
                    .into_any_element(),
            ]
        },
        |rows| {
            rows.into_iter()
                .map(|(name, value)| {
                    h_flex()
                        .gap_4()
                        .justify_between()
                        .child(
                            div()
                                .text_color(theme.muted_foreground)
                                .child(name.to_owned()),
                        )
                        .child(value)
                        .into_any_element()
                })
                .collect()
        },
    );
    v_flex()
        .min_w_48()
        .gap_1()
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.to_owned()),
        )
        .children(rows)
}
