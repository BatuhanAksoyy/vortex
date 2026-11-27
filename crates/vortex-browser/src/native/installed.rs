//! The page's `navigator.serial` and `navigator.hid`, answered by the test
//! in Vortex's place.

use std::time::Duration;

use async_channel::Receiver;
use futures::{executor::block_on, future::join};
use serde_json::{Value, json};

use crate::{
    Chrome, NativeCall, NativeError, Page, PageEvent, Profile, Viewport, find, test_server::serve,
};

const PAGE: &str = "<!doctype html><title>Ports</title><p>Ports";

async fn next_call(events: &Receiver<PageEvent>) -> NativeCall {
    while let Ok(event) = events.recv().await {
        if let PageEvent::NativeCall(call) = event {
            return call;
        }
    }
    panic!("the events ended");
}

/// What `expression` comes to once each call it makes is answered.
async fn answered(
    page: &Page,
    events: &Receiver<PageEvent>,
    expression: &str,
    answers: Vec<(&str, Result<Value, NativeError>)>,
) -> String {
    let answering = async {
        for (method, done) in answers {
            let call = next_call(events).await;
            assert_eq!(call.method, method);
            page.answer_native(&call, done);
        }
    };
    let (result, ()) = join(page.evaluate(expression), answering).await;
    result.unwrap()
}

async fn exercise(page: &Page, events: &Receiver<PageEvent>) {
    let port = json!({"id": "serial:/dev/x", "usbVendorId": 9025, "usbProductId": 67});
    let opened = "navigator.serial.requestPort().then(async p => { \
                  await p.open({baudRate: 9600}); window.port = p; \
                  return JSON.stringify(p.getInfo()); })";
    let answers = vec![("requestPort", Ok(port)), ("open", Ok(Value::Null))];
    let info = answered(page, events, opened, answers).await;
    let info: Value = serde_json::from_str(&info).unwrap();
    assert_eq!(info, json!({"usbVendorId": 9025, "usbProductId": 67}));

    let read = "port.readable.getReader().read().then(r => Array.from(r.value).join())";
    let reading = page.evaluate(read);
    let event = json!({"type": "data", "data": "AQID"});
    let sent = async {
        // The reader must be waiting before the data comes.
        std::thread::sleep(Duration::from_millis(200));
        page.native_event("serial", "serial:/dev/x", &event);
    };
    let (bytes, ()) = join(reading, sent).await;
    assert_eq!(bytes.unwrap(), "1,2,3");

    let refused = NativeError {
        name: "NotFoundError".into(),
        message: "No device selected.".into(),
    };
    let asked = "navigator.hid.requestDevice({filters: []}).then(d => d.length)";
    let none = answered(page, events, asked, vec![("requestDevice", Err(refused))]).await;
    assert_eq!(none, "0");
    let unfiltered = "navigator.usb.requestDevice({}).catch(e => e.name)";
    assert_eq!(page.evaluate(unfiltered).await.unwrap(), "TypeError");
}

#[test]
fn a_page_reaches_devices_through_vortex() {
    let Some(browser) = find() else {
        return;
    };
    let origin = serve(PAGE);
    let dir = tempfile::tempdir().unwrap();
    let profile = Profile::claim(&dir.path().join("profile")).unwrap();
    let chrome = Chrome::launch(&browser, &profile, 1.0).unwrap();
    let viewport = Viewport {
        width: 400,
        height: 300,
        phone: false,
    };
    let (page, events) = block_on(chrome.open_page(viewport)).unwrap();
    block_on(page.navigate(&format!("{origin}/"))).unwrap();
    while block_on(page.evaluate("document.title")).unwrap() != "Ports" {
        std::thread::sleep(Duration::from_millis(50));
    }
    block_on(exercise(&page, &events));
}
