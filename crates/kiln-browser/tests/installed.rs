//! Drives the browser installed on this machine, in a profile of its own.
//! Passes without checking anything where there is none.

#[cfg(test)]
mod installed {
    use std::{path::Path, time::Duration};

    use async_channel::Receiver;
    use futures::executor::block_on;
    use kiln_browser::{
        Button, Chrome, Frame, Key, Modifiers, MouseInput, MouseKind, Page, PageEvent, Viewport,
        find,
    };

    const VIEWPORT: Viewport = Viewport {
        width: 400,
        height: 300,
        phone: false,
    };
    const PAGE: &str = "<title>Kiln test</title><input autofocus><a href=/next target=_blank style='position:fixed;left:0;top:100px'>x</a><p>hello</p>";

    /// The browser ends with the test, and its events with it, so a hang is a
    /// failure within `limit`.
    fn end_after(limit: Duration) {
        std::thread::spawn(move || {
            std::thread::sleep(limit);
            std::process::exit(1);
        });
    }

    async fn first_frame(page: &Page, events: &Receiver<PageEvent>) -> Frame {
        loop {
            match events.recv().await.unwrap() {
                PageEvent::Opened => while page.show().await.is_err() {},
                PageEvent::Frame(frame) => return frame,
                _ => {}
            }
        }
    }

    /// Goes on once the page's scripts ran, which focus the field.
    async fn loaded(events: &Receiver<PageEvent>) {
        while events.recv().await.unwrap() != PageEvent::Loading(false) {}
    }

    fn key(name: &str, typed: bool, modifiers: Modifiers) -> Key {
        Key {
            name: name.to_owned(),
            text: typed.then(|| name.to_owned()),
            modifiers,
            repeated: false,
        }
    }

    async fn open_popup(page: &Page, events: &Receiver<PageEvent>) {
        for kind in [MouseKind::Pressed, MouseKind::Released] {
            page.mouse(MouseInput {
                kind,
                x: 4.,
                y: 108.,
                button: Some(Button::Left),
                clicks: 1,
                modifiers: Modifiers::default(),
            });
        }
        let popup = loop {
            if let PageEvent::Popup(url) = events.recv().await.unwrap() {
                break url;
            }
        };
        assert_eq!(popup, "file:///next");
    }

    async fn type_and_select(page: &Page) {
        for letter in ["k", "ğ"] {
            let key = key(letter, true, Modifiers::default());
            page.key_down(&key);
            page.key_up(&key);
        }
        let select = if cfg!(target_os = "macos") {
            Modifiers::META
        } else {
            Modifiers::CONTROL
        };
        page.key_down(&key("a", false, select));
        assert_eq!(page.selection().await.unwrap(), "kğ");
    }

    async fn drive(chrome: &Chrome, file: &Path) {
        let (page, events) = chrome.open_page(VIEWPORT).await.unwrap();
        let url = format!("file://{}", file.display());
        page.navigate(&url).await.unwrap();
        let frame = first_frame(&page, &events).await;
        assert_eq!(&frame.jpeg[..2], [0xff, 0xd8], "a JPEG");
        page.acknowledge(&frame);
        page.reload();
        loaded(&events).await;
        let history = page.history().await.unwrap();
        assert_eq!(history.title, "Kiln test");
        assert!(!history.can_go_back());
        type_and_select(&page).await;
        open_popup(&page, &events).await;
        assert!(page.navigate("http://127.0.0.1:1/").await.is_err());
    }

    #[test]
    fn a_page_is_drawn_and_typed_into() {
        let Some(browser) = find() else {
            return;
        };
        end_after(Duration::from_secs(60));
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("page.html");
        std::fs::write(&file, PAGE).unwrap();
        let chrome = Chrome::launch(&browser, &dir.path().join("profile"), 2.0).unwrap();
        block_on(drive(&chrome, &file));
    }
}
