//! What the page asks for, and the browser outside Kiln.

use gpui_kit::{
    Context, CursorStyle, PathPromptOptions, Window,
    component::{WindowExt, notification::Notification},
};
use kiln_browser::PageEvent;

use super::Browser;

impl Browser {
    pub(super) fn on_request(
        &mut self,
        event: PageEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            PageEvent::Dialog(dialog) => self.ask(dialog, window, cx),
            PageEvent::DialogClosed => self.dialog_closed(window, cx),
            PageEvent::Select(select) => self.open_select(&select, window, cx),
            PageEvent::ContextMenu(target) => self.open_context_menu(&target, window, cx),
            PageEvent::ChooseFiles { node, multiple } => self.choose_files(node, multiple, cx),
            PageEvent::DownloadStarted { id, name } => self.download_started(id, name, cx),
            PageEvent::Download(progress) => self.download_progressed(&progress, cx),
            PageEvent::Cursor(name) => {
                self.cursor = cursor(&name);
                cx.notify();
            }
            _ => {}
        }
    }

    fn choose_files(&self, node: i64, multiple: bool, cx: &mut Context<Self>) {
        let Some(page) = self.page().cloned() else {
            return;
        };
        let chosen = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple,
            prompt: None,
        });
        cx.spawn(async move |_, _| {
            if let Ok(Ok(Some(paths))) = chosen.await {
                page.give_files(node, &paths);
            }
        })
        .detach();
    }

    /// Opens the page in the browser the user browses with.
    pub(super) fn open_outside(&self, cx: &mut Context<Self>) {
        if self.url.starts_with("http") {
            cx.open_url(&self.url);
        }
    }

    pub(super) fn go_to_link(&mut self, url: &str, window: &mut Window, cx: &mut Context<Self>) {
        match kiln_browser::normalize(url) {
            Ok(url) => self.go_to(url, window, cx),
            Err(error) => {
                window.push_notification(Notification::warning(error.to_string()), cx);
            }
        }
    }
}

/// The cursor by its CSS name.
fn cursor(name: &str) -> CursorStyle {
    const STYLES: [(&str, CursorStyle); 24] = [
        ("pointer", CursorStyle::PointingHand),
        ("text", CursorStyle::IBeam),
        ("vertical-text", CursorStyle::IBeamCursorForVerticalLayout),
        ("crosshair", CursorStyle::Crosshair),
        ("cell", CursorStyle::Crosshair),
        ("grab", CursorStyle::OpenHand),
        ("grabbing", CursorStyle::ClosedHand),
        ("move", CursorStyle::OpenHand),
        ("not-allowed", CursorStyle::OperationNotAllowed),
        ("no-drop", CursorStyle::OperationNotAllowed),
        ("alias", CursorStyle::DragLink),
        ("copy", CursorStyle::DragCopy),
        ("context-menu", CursorStyle::ContextualMenu),
        ("col-resize", CursorStyle::ResizeColumn),
        ("row-resize", CursorStyle::ResizeRow),
        ("ew-resize", CursorStyle::ResizeLeftRight),
        ("e-resize", CursorStyle::ResizeRight),
        ("w-resize", CursorStyle::ResizeLeft),
        ("ns-resize", CursorStyle::ResizeUpDown),
        ("n-resize", CursorStyle::ResizeUp),
        ("s-resize", CursorStyle::ResizeDown),
        ("nwse-resize", CursorStyle::ResizeUpLeftDownRight),
        ("nesw-resize", CursorStyle::ResizeUpRightDownLeft),
        ("se-resize", CursorStyle::ResizeUpLeftDownRight),
    ];
    let style = STYLES.iter().find(|(css, _)| *css == name);
    style.map_or(CursorStyle::Arrow, |(_, style)| *style)
}
