//! Drives the browser installed on this machine, in a profile of its own.
//! Passes without checking anything where there is none.

#[cfg(test)]
mod installed {
    use std::{path::Path, time::Duration};

    use async_channel::Receiver;
    use futures::executor::block_on;
    use kiln_browser::{
        Button, Chrome, Dialog, DialogKind, Frame, Key, Modifiers, MouseInput, MouseKind, Page,
        PageEvent, Profile, Viewport, find,
    };

    const VIEWPORT: Viewport = Viewport {
        width: 400,
        height: 300,
        phone: false,
    };
    const PAGE: &str = "<title>Kiln test</title><input autofocus><a href=/next target=_blank style='position:fixed;left:0;top:100px'>x</a><select style='position:fixed;left:0;top:140px' onchange='prompt(value)'><option value=one>One<optgroup label=More><option value=two>Two</select><input type=file style='position:fixed;left:0;top:180px' onchange='alert(files[0].name)'><script>addEventListener('paste', e => alert(e.clipboardData.files[0].type))</script><p>hello</p>";

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
        click(page, 4., 108.);
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

    fn click(page: &Page, x: f32, y: f32) {
        for kind in [MouseKind::Pressed, MouseKind::Released] {
            page.mouse(MouseInput {
                kind,
                x,
                y,
                button: Some(Button::Left),
                clicks: 1,
                modifiers: Modifiers::default(),
            });
        }
    }

    async fn choose_and_answer(page: &Page, events: &Receiver<PageEvent>) {
        click(page, 20., 148.);
        let select = loop {
            if let PageEvent::Select(select) = events.recv().await.unwrap() {
                break select;
            }
        };
        assert_eq!(select.choices[1].label, "Two");
        assert_eq!(select.choices[1].group.as_deref(), Some("More"));
        page.choose(&select, 1);
        let dialog = asked(events).await;
        assert_eq!((dialog.kind, &*dialog.message), (DialogKind::Prompt, "two"));
        page.answer_dialog(true, "ok");
        while events.recv().await.unwrap() != PageEvent::DialogClosed {}
    }

    async fn asked(events: &Receiver<PageEvent>) -> Dialog {
        loop {
            if let PageEvent::Dialog(dialog) = events.recv().await.unwrap() {
                return dialog;
            }
        }
    }

    async fn hand_over(page: &Page, events: &Receiver<PageEvent>, file: &Path) {
        click(page, 20., 190.);
        let node = loop {
            if let PageEvent::ChooseFiles { node, .. } = events.recv().await.unwrap() {
                break node;
            }
        };
        page.give_files(node, &[file.to_path_buf()]);
        assert_eq!(asked(events).await.message, "page.html");
        page.answer_dialog(true, "");
        page.paste_image("image/png", b"not read");
        assert_eq!(asked(events).await.message, "image/png");
        page.answer_dialog(true, "");
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
        choose_and_answer(&page, &events).await;
        hand_over(&page, &events, file).await;
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
        let chrome = Chrome::launch(
            &browser,
            &Profile::claim(&dir.path().join("profile")).unwrap(),
            2.0,
        )
        .unwrap();
        block_on(drive(&chrome, &file));
    }
}
