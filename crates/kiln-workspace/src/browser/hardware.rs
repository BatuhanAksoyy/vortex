//! A page using `WebUSB`, `WebHID` or Web Serial. The page's script calls
//! Kiln instead of the browser, which reaches no device: Kiln asks the user
//! which device the site may have, keeps the answer until it quits, and
//! carries out each call off the UI thread.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use gpui_kit::prelude::*;
use gpui_kit::{App, AsyncApp, Context, Task, WeakEntity, Window};
use kiln_browser::{NativeCall, NativeError, Page};
use kiln_devices::{DeviceEvent, Devices, Found, Kind};
use serde_json::{Value, json};

use super::{Browser, device_prompt::NativeAsk, site_permissions::origin};

/// How often the devices a site was given are looked for, to tell the page
/// they were plugged in or out.
const WATCH: Duration = Duration::from_secs(2);
const KINDS: [Kind; 3] = [Kind::Usb, Kind::Hid, Kind::Serial];

pub(super) struct Hardware {
    /// Keeps the open chooser's list up to date.
    pub(super) chooser: Task<()>,
    /// Made on the first call; it holds what the page opened.
    devices: Option<Arc<Devices>>,
    /// The devices each site was given, by origin.
    granted: HashMap<String, HashSet<String>>,
    /// Those of the site's that were plugged in at the last look.
    present: HashSet<String>,
    events: Task<()>,
    watching: Task<()>,
}

impl Default for Hardware {
    fn default() -> Self {
        Self {
            chooser: Task::ready(()),
            devices: None,
            granted: HashMap::new(),
            present: HashSet::new(),
            events: Task::ready(()),
            watching: Task::ready(()),
        }
    }
}

pub(super) fn failure(name: &str, message: &str) -> NativeError {
    NativeError {
        name: name.to_owned(),
        message: message.to_owned(),
    }
}

/// What the page reads of a device: its fields, and Kiln's name for it.
fn shown(found: &Found) -> Value {
    let mut info = found.info.clone();
    info["id"] = json!(found.id);
    info
}

/// A call the page waits on, with what carries it out.
pub(super) struct Asked {
    pub page: Page,
    pub call: NativeCall,
    pub kind: Kind,
    pub devices: Arc<Devices>,
}

impl Asked {
    fn refuse(&self, name: &str, message: &str) {
        self.page
            .answer_native(&self.call, Err(failure(name, message)));
    }

    /// Carries the call out on the device, off the UI thread.
    fn carry_out(self, method: &str, cx: &App) {
        let method = method.to_owned();
        cx.background_spawn(async move {
            let Self {
                page,
                call,
                kind,
                devices,
            } = self;
            let device = call.device.as_deref().unwrap_or_default();
            let done = devices.call(kind, device, &method, &call.args).await;
            page.answer_native(
                &call,
                done.map_err(|error| failure(error.name, &error.message)),
            );
        })
        .detach();
    }

    /// Those of the devices plugged in that the site was given.
    fn list(self, granted: HashSet<String>, cx: &App) {
        cx.background_spawn(async move {
            let found = self.devices.find(self.kind, &Value::Null);
            let given = found.iter().filter(|it| granted.contains(&it.id));
            self.page
                .answer_native(&self.call, Ok(given.map(shown).collect()));
        })
        .detach();
    }
}

impl Browser {
    pub(super) fn on_native(
        &mut self,
        call: NativeCall,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(page) = self.page().cloned() else {
            return;
        };
        let Some(kind) = Kind::parse(&call.kind) else {
            page.answer_native(&call, Err(failure("NotSupportedError", "Unknown API.")));
            return;
        };
        let asked = Asked {
            page,
            call,
            kind,
            devices: self.devices(cx),
        };
        let Some(site) = origin(&self.url) else {
            asked.refuse("SecurityError", "Access denied.");
            return;
        };
        let granted = self.hardware.granted.entry(site).or_default();
        let device = asked.call.device.clone().unwrap_or_default();
        match asked.call.method.as_str() {
            "requestDevice" | "requestPort" => Self::find_for_chooser(asked, window, cx),
            "getDevices" | "getPorts" => asked.list(granted.clone(), cx),
            "forget" => {
                granted.remove(&device);
                asked.carry_out("close", cx);
            }
            method if granted.contains(&device) => {
                let method = method.to_owned();
                asked.carry_out(&method, cx);
            }
            _ => asked.refuse("SecurityError", "Access denied."),
        }
    }

    /// Gives the page the device chosen, or tells it none was.
    pub(super) fn answer_native_chooser(&mut self, ask: &NativeAsk, chosen: Option<&str>) {
        let Some(page) = self.page() else {
            return;
        };
        let found = chosen.and_then(|id| ask.found.iter().find(|found| found.id == id));
        let Some(found) = found else {
            let none = match ask.kind {
                Kind::Serial => "No port selected by the user.",
                Kind::Usb | Kind::Hid => "No device selected.",
            };
            page.answer_native(&ask.call, Err(failure("NotFoundError", none)));
            return;
        };
        page.answer_native(&ask.call, Ok(shown(found)));
        if let Some(site) = origin(&self.url) {
            self.hardware
                .granted
                .entry(site)
                .or_default()
                .insert(found.id.clone());
            self.hardware.present.insert(found.id.clone());
        }
    }

    /// The page's devices, with what they report sent on to it.
    fn devices(&mut self, cx: &mut Context<Self>) -> Arc<Devices> {
        if let Some(devices) = &self.hardware.devices {
            return devices.clone();
        }
        let (devices, events) = Devices::new();
        let devices = Arc::new(devices);
        self.hardware.devices = Some(devices.clone());
        self.hardware.events = cx.spawn(async move |this, cx| send_on(this, events, cx).await);
        self.hardware.watching = cx.spawn(async move |this, cx| watch(this, cx).await);
        devices
    }

    /// Tells the page which of the site's devices were plugged in or out.
    fn look_for_devices(&mut self, cx: &mut Context<Self>) {
        let (Some(devices), Some(page)) = (self.hardware.devices.clone(), self.page().cloned())
        else {
            return;
        };
        let Some(given) =
            origin(&self.url).and_then(|site| self.hardware.granted.get(&site).cloned())
        else {
            return;
        };
        let before = self.hardware.present.clone();
        let looking = cx.background_spawn(async move {
            let mut now = HashSet::new();
            for kind in KINDS {
                let present = devices
                    .present(kind)
                    .into_iter()
                    .filter(|id| given.contains(id));
                let present: HashSet<String> = present.collect();
                changed(&devices, &page, kind, &before, &present).await;
                now.extend(present);
            }
            now
        });
        cx.spawn(async move |this, cx| {
            let now = looking.await;
            let _ = this.update(cx, |this, _| this.hardware.present = now);
        })
        .detach();
    }

    /// Lets go of every device the page opened, as when it goes away.
    pub(super) fn close_devices(&mut self) {
        if let Some(devices) = &self.hardware.devices {
            devices.close_all();
        }
    }
}

async fn send_on(
    this: WeakEntity<Browser>,
    events: async_channel::Receiver<DeviceEvent>,
    cx: &mut AsyncApp,
) {
    while let Ok(DeviceEvent {
        kind,
        device,
        event,
    }) = events.recv().await
    {
        let send = |this: &mut Browser, _: &mut Context<Browser>| {
            if let Some(page) = this.page() {
                page.native_event(kind.name(), &device, &event);
            }
        };
        if this.update(cx, send).is_err() {
            return;
        }
    }
}

async fn watch(this: WeakEntity<Browser>, cx: &mut AsyncApp) {
    while this.update(cx, Browser::look_for_devices).is_ok() {
        cx.background_executor().timer(WATCH).await;
    }
}

/// Tells the page of the devices of `kind` that came or went.
async fn changed(
    devices: &Devices,
    page: &Page,
    kind: Kind,
    before: &HashSet<String>,
    now: &HashSet<String>,
) {
    let ours = |id: &&String| id.starts_with(kind.name());
    for gone in before.difference(now).filter(ours) {
        // Its handle would fail from now on.
        let _ = devices.call(kind, gone, "close", &Value::Null).await;
        page.native_event(kind.name(), gone, &json!({"type": "disconnect"}));
    }
    let came: Vec<&String> = now.difference(before).collect();
    if came.is_empty() {
        return;
    }
    for found in devices.find(kind, &Value::Null) {
        if came.contains(&&found.id) {
            page.native_event(
                kind.name(),
                &found.id,
                &json!({"type": "connect", "info": found.info}),
            );
        }
    }
}
