//! One page in the running browser.

use std::sync::Arc;

use anyhow::{Context as _, bail};
use async_channel::Receiver;
use serde_json::{Value, json};

use crate::{
    event::{Frame, PageEvent},
    input::{MouseInput, Wheel},
    keys::Key,
    transport::Transport,
};

/// Text stays sharp; a frame is some hundred kilobytes.
const JPEG_QUALITY: u8 = 90;
/// Wider than any panel, so frames are never scaled down.
const MAX_FRAME: u32 = 8192;

/// The selected text, in a field or on the page.
const SELECTION: &str = "(() => { const e = document.activeElement; \
    return e && typeof e.selectionStart === 'number' \
    ? e.value.slice(e.selectionStart, e.selectionEnd) \
    : String(getSelection()); })()";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
    /// Laid out as on a phone.
    pub phone: bool,
}

/// Where the page is and where it can go.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct History {
    pub url: String,
    pub title: String,
    back: Option<i64>,
    forward: Option<i64>,
}

impl History {
    #[must_use]
    pub fn can_go_back(&self) -> bool {
        self.back.is_some()
    }

    #[must_use]
    pub fn can_go_forward(&self) -> bool {
        self.forward.is_some()
    }

    fn parse(reply: &Value) -> Option<Self> {
        let entries = reply["entries"].as_array()?;
        let current = usize::try_from(reply["currentIndex"].as_u64()?).ok()?;
        let text = |key: &str| {
            entries[current][key]
                .as_str()
                .unwrap_or_default()
                .to_owned()
        };
        let id = |index: usize| entries.get(index)?["id"].as_i64();
        entries.get(current)?;
        // The blank page every tab starts with is nothing to go back to.
        let back = current
            .checked_sub(1)
            .filter(|index| *index > 0 || entries[*index]["url"] != "about:blank");
        Some(Self {
            url: text("url"),
            title: text("title"),
            back: back.and_then(id),
            forward: id(current + 1),
        })
    }
}

#[derive(Clone)]
pub struct Page {
    transport: Arc<Transport>,
    target: String,
    session: String,
    /// What the page is told the browser is.
    agent: String,
}

impl Page {
    pub(crate) async fn open(
        transport: Arc<Transport>,
        viewport: Viewport,
    ) -> anyhow::Result<(Self, Receiver<PageEvent>)> {
        // In a window of its own: a tab behind another one is not drawn.
        let create = json!({"url": "about:blank", "newWindow": true});
        let created = transport.call("Target.createTarget", create, None).await?;
        let target = text(&created, "targetId")?;
        let attach = json!({"targetId": target, "flatten": true});
        let attached = transport
            .call("Target.attachToTarget", attach, None)
            .await?;
        let session = text(&attached, "sessionId")?;
        let version = transport.call("Browser.getVersion", json!({}), None);
        // Without a window it names itself differently, and sites that
        // only know Chrome turn it away.
        let agent = text(&version.await?, "userAgent")?.replace("HeadlessChrome", "Chrome");
        let (sender, events) = async_channel::unbounded();
        transport.listen(&session, &target, sender);
        let page = Self {
            transport,
            target,
            session,
            agent,
        };
        page.send("Page.enable", &json!({}));
        page.send("Inspector.enable", &json!({}));
        // A window without focus draws no caret and hides menus on hover.
        page.send(
            "Emulation.setFocusEmulationEnabled",
            &json!({"enabled": true}),
        );
        // Nothing shows the system's file dialog yet; it would never close.
        page.send(
            "Page.setInterceptFileChooserDialog",
            &json!({"enabled": true}),
        );
        page.resize(viewport);
        Ok((page, events))
    }

    /// Goes to `url`, which `normalize` gave.
    pub async fn navigate(&self, url: &str) -> anyhow::Result<()> {
        let reply = self.call("Page.navigate", json!({"url": url})).await?;
        match reply["errorText"].as_str() {
            // The user left for another page before this one came.
            None | Some("net::ERR_ABORTED") => Ok(()),
            Some(error) => bail!("Cannot open {url}: {error}"),
        }
    }

    pub fn reload(&self) {
        self.send("Page.reload", &json!({}));
    }

    pub fn stop(&self) {
        self.send("Page.stopLoading", &json!({}));
    }

    pub async fn history(&self) -> anyhow::Result<History> {
        let reply = self.call("Page.getNavigationHistory", json!({})).await?;
        History::parse(&reply).context("the browser sent no history")
    }

    pub fn go_back(&self, history: &History) {
        self.go_to(history.back);
    }

    pub fn go_forward(&self, history: &History) {
        self.go_to(history.forward);
    }

    fn go_to(&self, entry: Option<i64>) {
        if let Some(entry) = entry {
            self.send("Page.navigateToHistoryEntry", &json!({"entryId": entry}));
        }
    }

    pub fn resize(&self, viewport: Viewport) {
        let agent = if viewport.phone {
            phone_agent(&self.agent)
        } else {
            self.agent.clone()
        };
        self.send(
            "Emulation.setUserAgentOverride",
            &json!({"userAgent": agent}),
        );
        let metrics = json!({
            "width": viewport.width,
            "height": viewport.height,
            // The browser's own, which it was started with.
            "deviceScaleFactor": 0,
            "mobile": viewport.phone,
        });
        self.send("Emulation.setDeviceMetricsOverride", &metrics);
        let touch = json!({"enabled": viewport.phone});
        self.send("Emulation.setEmitTouchEventsForMouse", &touch);
    }

    /// Asks for frames from now on; each must be acknowledged. Refused for a
    /// moment after the page opened another document.
    pub async fn show(&self) -> anyhow::Result<()> {
        // Frames the last document still sends would have it refused too.
        self.hide();
        let cast = json!({
            "format": "jpeg",
            "quality": JPEG_QUALITY,
            "maxWidth": MAX_FRAME,
            "maxHeight": MAX_FRAME,
        });
        self.call("Page.startScreencast", cast).await.map(drop)
    }

    pub fn hide(&self) {
        self.send("Page.stopScreencast", &json!({}));
    }

    /// Lets the browser send the next frame, so a slow reader gets fewer
    /// frames rather than late ones.
    pub fn acknowledge(&self, frame: &Frame) {
        let ack = json!({"sessionId": frame.ack});
        self.send("Page.screencastFrameAck", &ack);
    }

    pub fn mouse(&self, input: MouseInput) {
        self.send("Input.dispatchMouseEvent", &input.params());
    }

    pub fn wheel(&self, wheel: Wheel) {
        self.send("Input.dispatchMouseEvent", &wheel.params());
    }

    pub fn key_down(&self, key: &Key) {
        self.send("Input.dispatchKeyEvent", &key.down());
    }

    pub fn key_up(&self, key: &Key) {
        self.send("Input.dispatchKeyEvent", &key.up());
    }

    /// Shows `text` as being composed, before it is typed.
    pub fn compose(&self, text: &str) {
        let end = text.encode_utf16().count();
        let composition = json!({"text": text, "selectionStart": end, "selectionEnd": end});
        self.send("Input.imeSetComposition", &composition);
    }

    /// Types `text` at once, as pasting does.
    pub fn insert(&self, text: &str) {
        self.send("Input.insertText", &json!({"text": text}));
    }

    pub async fn selection(&self) -> anyhow::Result<String> {
        let evaluate = json!({"expression": SELECTION, "returnByValue": true});
        let reply = self.call("Runtime.evaluate", evaluate).await?;
        text(&reply["result"], "value")
    }

    pub fn answer_dialog(&self, accept: bool) {
        let answer = json!({"accept": accept});
        self.send("Page.handleJavaScriptDialog", &answer);
    }

    pub fn close(&self) {
        self.transport.forget(&self.session);
        let close = json!({"targetId": self.target});
        self.transport.send("Target.closeTarget", &close, None);
    }

    fn send(&self, method: &str, params: &Value) {
        self.transport.send(method, params, Some(&self.session));
    }

    async fn call(&self, method: &str, params: Value) -> anyhow::Result<Value> {
        self.transport
            .call(method, params, Some(&self.session))
            .await
    }
}

/// The same browser as it is on a phone, for sites with pages of their own
/// for phones.
fn phone_agent(agent: &str) -> String {
    let system = agent
        .split_once('(')
        .and_then(|(_, rest)| rest.split_once(')'))
        .map_or("", |(system, _)| system);
    if system.is_empty() {
        return agent.to_owned();
    }
    agent
        .replacen(system, "Linux; Android 10; K", 1)
        .replace(" Safari/", " Mobile Safari/")
}

fn text(value: &Value, key: &str) -> anyhow::Result<String> {
    let text = value[key].as_str();
    Ok(text
        .with_context(|| format!("the browser sent no {key}"))?
        .to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_phone_is_named_with_the_same_version() {
        let desktop = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/158.0.0.0 Safari/537.36";
        let phone = "Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/158.0.0.0 Mobile Safari/537.36";
        assert_eq!(phone_agent(desktop), phone);
    }

    #[test]
    fn the_first_blank_page_is_nothing_to_go_back_to() {
        let entry = |id: i64, url: &str| json!({"id": id, "url": url, "title": "T"});
        let entries = [
            entry(1, "about:blank"),
            entry(2, "http://a/"),
            entry(3, "http://b/"),
        ];
        let at = |index: usize| {
            History::parse(&json!({"currentIndex": index, "entries": entries})).unwrap()
        };
        assert!(!at(1).can_go_back());
        assert!(at(1).can_go_forward());
        assert!(at(2).can_go_back());
        assert!(!at(2).can_go_forward());
        assert_eq!(at(2).url, "http://b/");
        assert!(History::parse(&json!({"currentIndex": 9, "entries": entries})).is_none());
    }
}
