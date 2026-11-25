//! Chrome's own permissions, asked about through Kiln.

use std::time::Duration;

use async_channel::Receiver;
use futures::{
    executor::block_on,
    future::{Either, join, select},
};
use serde_json::json;

use crate::{Chrome, Page, PageEvent, Permission, Profile, Viewport, find, test_server::serve};

const PAGE: &str = "<!doctype html><title>Leave</title><p>Leave";
const LIMIT: Duration = Duration::from_secs(30);

/// Answers the first request, which must be for `expected`.
async fn answer(
    page: &Page,
    events: &Receiver<PageEvent>,
    origin: &str,
    expected: &[Permission],
    granted: bool,
) {
    while let Ok(event) = events.recv().await {
        if let PageEvent::Permission(request) = event {
            assert_eq!(request.permissions, expected);
            page.permit(&request, origin, granted).await.unwrap();
            return;
        }
    }
}

/// What `expression` comes to once its request is answered.
async fn asked(
    page: &Page,
    events: &Receiver<PageEvent>,
    origin: &str,
    (expression, expected, granted): (&str, &[Permission], bool),
) -> String {
    let answered = answer(page, events, origin, expected, granted);
    let (result, ()) = join(page.evaluate(expression), answered).await;
    result.unwrap()
}

async fn notified(events: &Receiver<PageEvent>) -> (String, String) {
    while let Ok(event) = events.recv().await {
        if let PageEvent::Notification { title, body } = event {
            return (title, body);
        }
    }
    panic!("the events ended");
}

async fn exercise(page: &Page, events: &Receiver<PageEvent>, origin: &str) {
    let query = "navigator.permissions.query({name: 'geolocation'}).then(s => s.state)";
    for permission in Permission::ALL {
        page.set_permission(None, permission, None).await.unwrap();
    }
    assert_eq!(page.evaluate(query).await.unwrap(), "prompt");
    notify_after_leave(page, events, origin).await;
    locate_after_leave(page, events, origin).await;
    film_after_refusal(page, events, origin).await;
}

async fn notify_after_leave(page: &Page, events: &Receiver<PageEvent>, origin: &str) {
    let notify = "Notification.requestPermission()";
    let notifications = [Permission::Notifications];
    let result = asked(page, events, origin, (notify, &notifications, true)).await;
    assert_eq!(result, "granted");
    page.evaluate("new Notification('Hi', {body: 'there'}), 1")
        .await
        .unwrap();
    assert_eq!(
        notified(events).await,
        ("Hi".to_owned(), "there".to_owned())
    );
}

async fn locate_after_leave(page: &Page, events: &Receiver<PageEvent>, origin: &str) {
    let place = json!({"latitude": 41.0, "longitude": 29.0, "accuracy": 10});
    page.call("Emulation.setGeolocationOverride", place)
        .await
        .unwrap();
    let locate = "new Promise(r => navigator.geolocation.getCurrentPosition(\
                  p => r(String(p.coords.latitude)), e => r('error ' + e.code)))";
    let location = [Permission::Geolocation];
    let result = asked(page, events, origin, (locate, &location, true)).await;
    assert_eq!(result, "41");
}

async fn film_after_refusal(page: &Page, events: &Receiver<PageEvent>, origin: &str) {
    let film =
        "navigator.mediaDevices.getUserMedia({video: true}).then(() => 'filming', e => e.name)";
    let camera = [Permission::Camera];
    // Without a camera, it fails either way; what counts is what was set.
    let result = asked(page, events, origin, (film, &camera, false)).await;
    assert_ne!(result, "filming");
    let state = "navigator.permissions.query({name: 'camera'}).then(s => s.state)";
    assert_eq!(page.evaluate(state).await.unwrap(), "denied");
}

/// Ends after `LIMIT`: a page that never asks would hang the test.
fn deadline() -> Receiver<()> {
    let (late, too_late) = async_channel::bounded(1);
    std::thread::spawn(move || {
        std::thread::sleep(LIMIT);
        let _ = late.send_blocking(());
    });
    too_late
}

#[test]
fn a_page_goes_on_once_the_user_answers() {
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
    while block_on(page.evaluate("document.title")).unwrap() != "Leave" {
        std::thread::sleep(Duration::from_millis(50));
    }
    let too_late = deadline();
    let late = std::pin::pin!(too_late.recv());
    let exercised = std::pin::pin!(exercise(&page, &events, &origin));
    if let Either::Right(_) = block_on(select(exercised, late)) {
        panic!("the page did not go on");
    }
}
