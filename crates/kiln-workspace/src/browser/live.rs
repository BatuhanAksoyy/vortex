//! Opening a page, following it, and letting go of it.

use std::{path::Path, sync::Arc, time::Duration};

use async_channel::Receiver;
use gpui_kit::prelude::*;
use gpui_kit::{
    AsyncWindowContext, BackgroundExecutor, Context, Image, ImageFormat, Task, WeakEntity, Window,
    component::{WindowExt, notification::Notification},
};
use kiln_browser::{Chrome, Frame, Page, PageEvent};

use super::{Browser, host};

/// How long a page nobody sees stays open.
const KEEP_HIDDEN: Duration = Duration::from_mins(10);
const SHOW_AGAIN: Duration = Duration::from_millis(30);
const SHOW_ATTEMPTS: usize = 100;

pub(super) enum State {
    Blank,
    Opening,
    /// Closed for going unseen, to be opened again when shown.
    PutAway,
    Open(Open),
    Failed(String),
}

/// An open page, which keeps the browser running.
pub(super) struct Open {
    chrome: Arc<Chrome>,
    pub page: Page,
}

impl Drop for Open {
    fn drop(&mut self) {
        self.page.close();
    }
}

impl Browser {
    pub(super) fn chrome_pid(&self) -> Option<u32> {
        match &self.state {
            State::Open(open) => open.chrome.pid(),
            State::Blank | State::Opening | State::PutAway | State::Failed(_) => None,
        }
    }

    pub(super) fn page(&self) -> Option<&Page> {
        match &self.state {
            State::Open(open) => Some(&open.page),
            State::Blank | State::Opening | State::PutAway | State::Failed(_) => None,
        }
    }

    /// Goes to `url`, starting the browser if it does not run.
    pub(super) fn go_to(&mut self, url: String, window: &mut Window, cx: &mut Context<Self>) {
        self.url.clone_from(&url);
        if let Some(page) = self.page().cloned() {
            self.navigation = cx.background_spawn(navigate(page, url));
        } else {
            self.open(url, window, cx);
        }
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn open(&mut self, url: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(profile) = self.profile.clone() else {
            self.fail("There is no folder to keep the browser's data in.", cx);
            return;
        };
        self.state = State::Opening;
        self.follow = cx.spawn_in(window, async move |this, cx| {
            let (open, events) = match connect(&this, &profile, cx).await {
                Ok(connected) => connected,
                Err(error) => {
                    let _ = this.update(cx, |this, cx| this.fail(&format!("{error:#}"), cx));
                    return;
                }
            };
            let shown = this.update_in(cx, |this, window, cx| this.opened(open, url, window, cx));
            if shown.is_err() {
                return;
            }
            while let Ok(event) = events.recv().await
                && handle(&this, event, cx).await.is_ok()
            {}
            let _ = this.update(cx, |this, cx| this.fail(CLOSED, cx));
        });
        cx.notify();
    }

    fn opened(&mut self, open: Open, url: String, window: &mut Window, cx: &mut Context<Self>) {
        if host::note_profile(cx) {
            window.push_notification(Notification::info(OTHER_PROFILE), cx);
        }
        // The panel may have been sized meanwhile.
        open.page.resize(self.viewport);
        self.restore_permissions(open.page.clone(), cx);
        self.navigation = cx.background_spawn(navigate(open.page.clone(), url));
        self.state = State::Open(open);
        cx.notify();
    }

    fn fail(&mut self, message: &str, cx: &mut Context<Self>) {
        self.state = State::Failed(message.to_owned());
        self.loading = false;
        self.menu = None;
        self.dialog = None;
        self.devices = None;
        self.drop_permission_requests();
        self.picking.on = false;
        self.unpainted.clear();
        self.ahead = false;
        cx.notify();
    }

    /// Shows the page again, or stops drawing it; unseen for long, it closes
    /// and is opened anew when next shown.
    pub fn set_shown(&mut self, shown: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.shown == shown {
            return;
        }
        self.shown = shown;
        self.unpainted.clear();
        self.ahead = false;
        self.close_later = Task::ready(());
        if !shown {
            self.showing = Task::ready(());
            if let Some(page) = self.page() {
                page.hide();
            }
            self.close_later = cx.spawn_in(window, async move |this, cx| {
                cx.background_executor().timer(KEEP_HIDDEN).await;
                let _ = this.update_in(cx, Self::put_away);
            });
        } else if self.page().is_some() {
            self.show(cx);
        } else if matches!(self.state, State::PutAway) {
            self.open(self.url.clone(), window, cx);
        }
    }

    fn show(&mut self, cx: &Context<Self>) {
        let Some(page) = self.page().cloned() else {
            return;
        };
        let executor = cx.background_executor().clone();
        self.showing = cx.background_spawn(show(page, executor));
    }

    fn put_away(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.page().is_none() {
            return;
        }
        self.follow = Task::ready(());
        self.navigation = Task::ready(());
        self.state = State::PutAway;
        self.menu = None;
        self.dialog = None;
        self.picking.on = false;
        if let Some((old, _)) = self.picture.take() {
            cx.drop_image(old, Some(window));
        }
    }

    fn on_event(&mut self, event: PageEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            PageEvent::Opened => {
                self.menu = None;
                // The document that asked is gone.
                self.devices = None;
                self.drop_permission_requests();
                self.set_picking(false, cx);
                if self.shown {
                    self.show(cx);
                }
                self.read_history(window, cx);
                self.storage_changed(cx);
            }
            PageEvent::Navigated => self.read_history(window, cx),
            PageEvent::Loading(loading) => {
                // Before the new document runs.
                if loading {
                    self.console.clear();
                    self.clear_network();
                }
                self.loading = loading;
                cx.notify();
            }
            PageEvent::Popup(url) => self.go_to(url, window, cx),
            PageEvent::Crashed => self.fail("The page crashed.", cx),
            request => self.on_request(request, window, cx),
        }
    }

    fn read_history(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(page) = self.page().cloned() else {
            return;
        };
        self.reading = cx.spawn_in(window, async move |this, cx| {
            let Ok(history) = page.history().await else {
                return;
            };
            let _ = this.update_in(cx, |this, window, cx| {
                this.url.clone_from(&history.url);
                this.history = history;
                this.show_url(window, cx);
                cx.notify();
            });
        });
    }

    fn draw(
        &mut self,
        frame: Frame,
        image: Arc<gpui_kit::RenderImage>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let State::Open(open) = &self.state else {
            return;
        };
        let scale = open.chrome.scale();
        // One frame ahead of the window keeps the browser drawing meanwhile;
        // more than one, and it draws what is never seen.
        if self.ahead {
            self.unpainted.push(frame);
        } else {
            open.page.acknowledge(&frame);
            self.ahead = true;
        }
        if let Some((old, _)) = self.picture.replace((image, scale)) {
            cx.drop_image(old, Some(window));
        }
        cx.notify();
    }
}

const CLOSED: &str = "The browser ended. Enter an address to start it again.";
const OTHER_PROFILE: &str = "Another Kiln is open, so this one's browser keeps its own sign-ins.";
/// The last browser may still be closing, with the profile in use.
const CONNECT_ATTEMPTS: usize = 8;
const CONNECT_AGAIN: Duration = Duration::from_millis(500);

async fn connect(
    this: &WeakEntity<Browser>,
    profile: &Path,
    cx: &mut AsyncWindowContext,
) -> anyhow::Result<(Open, Receiver<PageEvent>)> {
    for _ in 0..CONNECT_ATTEMPTS {
        let (scale, viewport) = this.read_with(cx, |this, _| (this.scale, this.viewport))?;
        let chrome = cx.update(|_, cx| host::chrome(profile, scale, cx))??;
        match chrome.open_page(viewport).await {
            Ok((page, events)) => return Ok((Open { chrome, page }, events)),
            Err(error) if !chrome.ended() => return Err(error),
            Err(_) => cx.background_executor().timer(CONNECT_AGAIN).await,
        }
    }
    anyhow::bail!(CLOSED)
}

async fn show(page: Page, executor: BackgroundExecutor) {
    for _ in 0..SHOW_ATTEMPTS {
        if page.show().await.is_ok() {
            return;
        }
        executor.timer(SHOW_AGAIN).await;
    }
    log::warn!("the browser does not draw the page");
}

/// Failures to load show on the page itself.
async fn navigate(page: Page, url: String) {
    if let Err(error) = page.navigate(&url).await {
        log::debug!("{error:#}");
    }
}

async fn handle(
    this: &WeakEntity<Browser>,
    event: PageEvent,
    cx: &mut AsyncWindowContext,
) -> anyhow::Result<()> {
    let PageEvent::Frame(frame) = event else {
        return this.update_in(cx, |this, window, cx| this.on_event(event, window, cx));
    };
    let svg = cx.update(|_, cx| cx.svg_renderer())?;
    let (frame, image) = cx
        .background_executor()
        .spawn(async move {
            let jpeg = Image::from_bytes(ImageFormat::Jpeg, frame.jpeg.clone());
            let image = jpeg.to_image_data(svg);
            (frame, image)
        })
        .await;
    let image = match image {
        Ok(image) => image,
        Err(error) => {
            log::warn!("cannot read a picture of the page: {error:#}");
            return this.update(cx, |this, _| this.unpainted.push(frame));
        }
    };
    this.update_in(cx, |this, window, cx| this.draw(frame, image, window, cx))
}
