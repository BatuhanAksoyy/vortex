//! The processes page of this chat, opened from the meter in its header
//! over the transcript and composer, like the subagents view.

use gpui_kit::prelude::*;
use gpui_kit::{
    Context, Entity, Focusable as _, Subscription, TestSupportExt, Window, component::ActiveTheme,
    div,
};

use super::ChatView;
use crate::processes::{ProcessMeter, ProcessMeterEvent, ProcessesDialog, ProcessesDialogEvent};

impl ChatView {
    /// Opens the page when the header's meter is pressed.
    pub(super) fn follow_meter(
        meter: &Entity<ProcessMeter>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Subscription {
        cx.subscribe_in(meter, window, |this, _, event, window, cx| match event {
            ProcessMeterEvent::Open => this.open_processes(window, cx),
        })
    }

    fn open_processes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(page) = ProcessesDialog::for_chat(cx.entity().downgrade(), window, cx) else {
            return;
        };
        let closed = cx.subscribe_in(&page, window, |this, _, event, window, cx| match event {
            ProcessesDialogEvent::Close => this.close_processes(window, cx),
        });
        ProcessesDialog::focus(&page, window, cx);
        self.processes_page = Some((page, closed));
        cx.notify();
    }

    fn close_processes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.processes_page.take().is_some() {
            let composer = self.composer.read(cx).focus_handle(cx);
            window.focus(&composer, cx);
            cx.notify();
        }
    }

    /// The page over this chat; the backdrop keeps clicks from reaching
    /// the chat under it.
    pub(super) fn render_processes(&self, cx: &Context<Self>) -> Option<impl IntoElement + use<>> {
        let (page, _) = self.processes_page.as_ref()?;
        Some(
            div()
                .id("chat-processes-overlay")
                .test_support()
                .absolute()
                .inset_0()
                .p_3()
                .occlude()
                .bg(cx.theme().overlay)
                .child(page.clone()),
        )
    }
}
