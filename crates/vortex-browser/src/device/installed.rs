//! Chrome's own Web Bluetooth, with a device it is told is near.

use std::time::Duration;

use async_channel::Receiver;
use futures::{
    executor::block_on,
    future::{Either, join, select},
};
use serde_json::json;

use crate::{Chrome, Page, PageEvent, Profile, Viewport, find};

const PAGE: &str = "<!doctype html><title>Pair</title><p>Pair";
const ASK: &str = "navigator.bluetooth.requestDevice({acceptAllDevices: true})\
                   .then(device => device.name, error => error.name)";
const ADDRESS: &str = "09:09:09:09:09:09";
const LIMIT: Duration = Duration::from_secs(30);

async fn place_device(page: &Page) {
    // Bluetooth is the browser's, not the page's.
    let browser = |method, params| page.transport.call(method, params, None);
    let on = json!({"state": "powered-on", "leSupported": true});
    browser("BluetoothEmulation.enable", on).await.unwrap();
    let near = json!({
        "address": ADDRESS,
        "name": "Vortex Heart",
        "manufacturerData": [],
        "knownServiceUuids": [],
    });
    let placed = browser("BluetoothEmulation.simulatePreconnectedPeripheral", near);
    placed.await.unwrap();
}

/// Picks the first device the browser lists.
async fn pick_first(page: &Page, events: &Receiver<PageEvent>) {
    while let Ok(event) = events.recv().await {
        let PageEvent::DeviceRequest(request) = event else {
            continue;
        };
        if let Some(device) = request.devices.first() {
            assert_eq!(device.id, ADDRESS);
            page.pick_device(&request.id, &device.id);
            return;
        }
    }
}

/// Ends after `LIMIT`: a browser that never asks would hang the test.
fn deadline() -> Receiver<()> {
    let (late, too_late) = async_channel::bounded(1);
    std::thread::spawn(move || {
        std::thread::sleep(LIMIT);
        let _ = late.send_blocking(());
    });
    too_late
}

async fn pair(page: &Page, events: &Receiver<PageEvent>) -> String {
    place_device(page).await;
    let (paired, ()) = join(page.evaluate(ASK), pick_first(page, events)).await;
    paired.unwrap()
}

#[test]
fn a_page_gets_the_device_the_user_picks() {
    let Some(browser) = find() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("pair.html");
    std::fs::write(&file, PAGE).unwrap();
    let profile = Profile::claim(&dir.path().join("profile")).unwrap();
    let chrome = Chrome::launch(&browser, &profile, 1.0).unwrap();
    let viewport = Viewport {
        width: 400,
        height: 300,
        phone: false,
    };
    let (page, events) = block_on(chrome.open_page(viewport)).unwrap();
    block_on(page.navigate(&format!("file://{}", file.display()))).unwrap();
    let too_late = deadline();
    let late = std::pin::pin!(too_late.recv());
    let pairing = std::pin::pin!(pair(&page, &events));
    let paired = block_on(select(pairing, late));
    match paired {
        Either::Left((name, _)) => assert_eq!(name, "Vortex Heart"),
        Either::Right(_) => panic!("the browser did not ask for a device"),
    }
}
