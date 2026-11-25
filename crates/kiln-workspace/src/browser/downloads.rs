//! The files the page sent, listed under it.

use std::path::{Path, PathBuf};

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, Context, SharedString, TestSupportExt,
    assets::IconName as Lucide,
    component::{
        ActiveTheme, Icon, IconName, Sizable as _,
        button::{Button, ButtonVariants as _},
        h_flex,
        progress::Progress,
        v_flex,
    },
    div, px,
};
use kiln_browser::{DownloadProgress, DownloadState};
use kiln_chat::scale::{ScaledButton as _, ScaledIcon as _};

use super::Browser;

/// The newest ones; a page can send any number.
const MOST: usize = 5;

pub(super) struct Download {
    id: String,
    name: String,
    stage: Stage,
}

enum Stage {
    /// How much arrived, of 100, if the site says how much there is.
    Arriving(Option<f32>),
    Kept(PathBuf),
    Failed(String),
}

impl Browser {
    pub(super) fn download_started(&mut self, id: String, name: String, cx: &mut Context<Self>) {
        let stage = Stage::Arriving(None);
        self.downloads.push(Download { id, name, stage });
        cx.notify();
    }

    pub(super) fn download_progressed(
        &mut self,
        progress: &DownloadProgress,
        cx: &mut Context<Self>,
    ) {
        let Some(at) = self.downloads.iter().position(|it| it.id == progress.id) else {
            return;
        };
        match progress.state {
            DownloadState::Running => {
                self.downloads[at].stage = Stage::Arriving(share(progress));
            }
            DownloadState::Canceled => {
                self.downloads[at].stage = Stage::Failed("Canceled".to_owned());
            }
            DownloadState::Received => self.keep(at, cx),
        }
        cx.notify();
    }

    fn keep(&self, at: usize, cx: &mut Context<Self>) {
        let Some(page) = self.page().cloned() else {
            return;
        };
        let (id, name) = (
            self.downloads[at].id.clone(),
            self.downloads[at].name.clone(),
        );
        let kept = cx.background_spawn({
            let id = id.clone();
            async move {
                let folder = kiln_browser::downloads_dir();
                let folder = folder.unwrap_or_else(std::env::temp_dir);
                page.keep_download(&id, &folder, &name)
            }
        });
        cx.spawn(async move |this, cx| {
            let kept = kept.await;
            let _ = this.update(cx, |this, cx| this.kept(&id, kept, cx));
        })
        .detach();
    }

    fn kept(&mut self, id: &str, kept: anyhow::Result<PathBuf>, cx: &mut Context<Self>) {
        if let Some(download) = self.downloads.iter_mut().find(|it| it.id == id) {
            download.stage = match kept {
                Ok(path) => Stage::Kept(path),
                Err(error) => Stage::Failed(format!("{error:#}")),
            };
        }
        cx.notify();
    }

    fn forget_download(&mut self, id: &str, cx: &mut Context<Self>) {
        let running = |it: &Download| it.id == id && matches!(it.stage, Stage::Arriving(_));
        if self.downloads.iter().any(running)
            && let Some(page) = self.page()
        {
            page.cancel_download(id);
        }
        self.downloads.retain(|it| it.id != id);
        cx.notify();
    }

    pub(super) fn render_downloads(&self, cx: &Context<Self>) -> Option<AnyElement> {
        if self.downloads.is_empty() {
            return None;
        }
        let newest = self.downloads.iter().rev().take(MOST);
        let list = v_flex()
            .id("browser-downloads")
            .test_support()
            .flex_none()
            .border_t_1()
            .border_color(cx.theme().border)
            .children(newest.map(|download| render_download(download, cx)));
        Some(list.into_any_element())
    }
}

fn share(progress: &DownloadProgress) -> Option<f32> {
    if progress.total == 0 {
        return None;
    }
    let percent = progress.received.saturating_mul(100) / progress.total;
    Some(f32::from(u8::try_from(percent.min(100)).unwrap_or(100)))
}

fn render_download(download: &Download, cx: &Context<Browser>) -> AnyElement {
    let theme = cx.theme();
    let id = |part: &str| SharedString::from(format!("browser-download-{part}-{}", download.id));
    let status = match &download.stage {
        Stage::Arriving(Some(percent)) => div()
            .w(px(96.))
            .child(Progress::new(id("progress")).value(*percent))
            .into_any_element(),
        Stage::Arriving(None) => note("Downloading…", cx),
        Stage::Kept(path) => reveal(id("show"), path),
        Stage::Failed(reason) => note(reason, cx),
    };
    let (forget, tooltip) = (
        download.id.clone(),
        match download.stage {
            Stage::Arriving(_) => "Cancel Download",
            Stage::Kept(_) | Stage::Failed(_) => "Remove from List",
        },
    );
    h_flex()
        .gap_2()
        .px_2()
        .py_1()
        .text_sm()
        .child(
            Icon::new(Lucide::Download)
                .in_row()
                .text_color(theme.muted_foreground),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .child(download.name.clone()),
        )
        .child(status)
        .child(
            Button::new(id("forget"))
                .ghost()
                .row_icon(IconName::Close)
                .tooltip(tooltip)
                .on_click(cx.listener(move |this, _, _, cx| this.forget_download(&forget, cx))),
        )
        .into_any_element()
}

fn note(text: &str, cx: &Context<Browser>) -> AnyElement {
    div()
        .max_w(px(200.))
        .truncate()
        .text_color(cx.theme().muted_foreground)
        .child(text.to_owned())
        .into_any_element()
}

fn reveal(id: SharedString, path: &Path) -> AnyElement {
    let path = path.to_path_buf();
    let label = if cfg!(target_os = "macos") {
        "Show in Finder"
    } else {
        "Show in Folder"
    };
    Button::new(id)
        .ghost()
        .small()
        .label(label)
        .on_click(move |_, _, cx| cx.reveal_path(&path))
        .into_any_element()
}
