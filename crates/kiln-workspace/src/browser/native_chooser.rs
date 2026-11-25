//! The chooser for a USB, HID or serial device: the devices that match
//! the page's filters, kept up to date while it is open, as they are
//! plugged in or out.

use std::{collections::HashSet, sync::Arc, time::Duration};

use gpui_kit::prelude::*;
use gpui_kit::{AsyncApp, Context, WeakEntity, Window};
use kiln_browser::{Device, DeviceRequest};
use kiln_devices::{Devices, Found, Kind};
use serde_json::Value;

use super::{
    Browser,
    device_prompt::NativeAsk,
    hardware::{Asked, failure},
};

/// How often the devices plugged in are looked for while it is open.
const RELIST: Duration = Duration::from_secs(1);

fn plugged_in(devices: &Devices, kind: Kind) -> HashSet<String> {
    devices.present(kind).into_iter().collect()
}

pub(super) fn listed(found: &[Found]) -> Vec<Device> {
    let device = |found: &Found| Device {
        id: found.id.clone(),
        name: found.name.clone(),
    };
    found.iter().map(device).collect()
}

impl Browser {
    pub(super) fn find_for_chooser(asked: Asked, window: &Window, cx: &mut Context<Self>) {
        let Asked {
            call,
            kind,
            devices,
            ..
        } = asked;
        let filters = call.args["filters"].clone();
        let looking = devices.clone();
        let finding = cx.background_spawn(async move {
            (plugged_in(&looking, kind), looking.find(kind, &filters))
        });
        cx.spawn_in(window, async move |this, cx| {
            let (seen, found) = finding.await;
            let watch = Watch {
                filters: call.args["filters"].clone(),
                devices,
                seen,
            };
            let ask = NativeAsk { call, kind, found };
            let _ = this.update_in(cx, |this, window, cx| {
                this.ask_native(ask, watch, window, cx);
            });
        })
        .detach();
    }

    /// Shows the chooser, unless another one is open.
    fn ask_native(
        &mut self,
        ask: NativeAsk,
        watch: Watch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.devices.is_some() {
            if let Some(page) = self.page() {
                let busy = failure("InvalidStateError", "A device chooser is already open.");
                page.answer_native(&ask.call, Err(busy));
            }
            return;
        }
        let kind = ask.kind;
        let request = DeviceRequest {
            id: String::new(),
            devices: listed(&ask.found),
        };
        self.open_device_prompt(request, Some(ask), window, cx);
        self.hardware.chooser = cx.spawn(async move |this, cx| relist(this, kind, watch, cx).await);
    }
}

/// What the open chooser lists, and how to look again.
struct Watch {
    filters: Value,
    devices: Arc<Devices>,
    /// The devices plugged in when it was last listed.
    seen: HashSet<String>,
}

/// Lists the devices again whenever those plugged in change, until the
/// chooser closes.
async fn relist(this: WeakEntity<Browser>, kind: Kind, watch: Watch, cx: &mut AsyncApp) {
    let Watch {
        filters,
        devices,
        mut seen,
    } = watch;
    loop {
        cx.background_executor().timer(RELIST).await;
        let open = this.read_with(cx, |this, _| this.native_chooser_open());
        if !open.unwrap_or(false) {
            return;
        }
        let looking = devices.clone();
        let now = cx
            .background_spawn(async move { plugged_in(&looking, kind) })
            .await;
        if now == seen {
            continue;
        }
        seen = now;
        let (looking, filters) = (devices.clone(), filters.clone());
        let found = cx
            .background_spawn(async move { looking.find(kind, &filters) })
            .await;
        let _ = this.update(cx, |this, cx| this.relist_native(found, cx));
    }
}
