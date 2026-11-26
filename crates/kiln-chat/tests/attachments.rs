//! Pictures and files sent with a message: pasted into the box, shown as
//! cards until sent, then shown in the transcript and sent to the agent.
//! Pictures a tool returns show under its row.

// Inside a `cfg(test)` module so helpers may `expect`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod attachments {
    use std::path::PathBuf;

    use gpui_kit::test::{TestQueryExt, TestWindowExt};
    use gpui_kit::{
        ClipboardEntry, ClipboardItem, ExternalPaths, Image, ImageFormat, TestAppContext,
    };
    use kiln_agent::{AgentEvent, Message, Picture, SessionCommand, ToolCall};
    use kiln_chat::Status;
    use serde_json::json;

    use super::common::Harness;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";

    fn paste(harness: &Harness, cx: &mut TestAppContext, entry: ClipboardEntry) {
        cx.write_to_clipboard(ClipboardItem {
            entries: vec![entry],
        });
        harness.with_window(cx, |window, cx| window.click("chat-composer", cx));
        harness.with_window(cx, |window, cx| window.press("cmd-v", cx));
    }

    #[gpui_kit::test]
    fn a_pasted_picture_is_sent_with_the_message(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        let image = Image::from_bytes(ImageFormat::Png, PNG.to_vec());
        paste(&harness, cx, ClipboardEntry::Image(image));
        harness.with_window(cx, |window, _| {
            assert!(window.find("chat-attachment-0").visible());
        });

        harness.type_and_send(cx, "what is this?");
        let picture = Picture {
            media_type: "image/png".into(),
            data: PNG.into(),
        };
        let sent = Message {
            text: "what is this?".into(),
            pictures: vec![picture],
            files: Vec::new(),
        };
        assert_eq!(harness.commands(), [SessionCommand::Send(sent)]);
        assert_eq!(harness.status(cx), Status::Running);
        harness.with_window(cx, |window, _| {
            assert!(
                window.try_find("chat-attachment-0").is_none(),
                "sent, so gone"
            );
            assert!(
                window.find_by_label("image/png").visible(),
                "shown in the message"
            );
        });
    }

    #[gpui_kit::test]
    fn copied_files_go_by_name_and_need_no_text(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        let file = PathBuf::from("/tmp/kiln-attach-test/notes.txt");
        let paths = ExternalPaths(vec![file.clone()].into());
        paste(&harness, cx, ClipboardEntry::ExternalPaths(paths));
        harness.with_window(cx, |window, cx| {
            assert!(window.find("chat-attachment-0").visible());
            window.press("enter", cx);
        });
        let sent = Message {
            text: String::new(),
            pictures: Vec::new(),
            files: vec![file],
        };
        assert_eq!(harness.commands(), [SessionCommand::Send(sent)]);
    }

    #[gpui_kit::test]
    fn a_picture_a_tool_returned_shows_under_its_row(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.type_and_send(cx, "read the picture");
        harness.emit(
            cx,
            AgentEvent::ToolCall(ToolCall {
                id: "read".into(),
                parent: None,
                name: "Read".into(),
                input: json!({ "file_path": "shot.png" }),
            }),
        );
        harness.emit(
            cx,
            AgentEvent::ToolResult {
                id: "read".into(),
                output: String::new(),
                pictures: vec![Picture {
                    media_type: "image/png".into(),
                    data: PNG.into(),
                }],
                is_error: false,
                patch: Vec::new(),
            },
        );
        harness.with_window(cx, |window, _| {
            assert!(window.find_by_label("image/png").visible());
        });
    }

    #[gpui_kit::test]
    fn a_click_shows_a_picture_at_full_size_until_escape(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        let image = Image::from_bytes(ImageFormat::Png, PNG.to_vec());
        paste(&harness, cx, ClipboardEntry::Image(image));
        harness.type_and_send(cx, "look");
        harness.with_window(cx, |window, cx| {
            window.click("user-picture-1-0", cx);
        });
        harness.with_window(cx, |window, cx| {
            let viewer = window.find("chat-picture-viewer").bounds();
            assert_eq!(viewer.size, window.viewport_size(), "the whole window");
            window.click("chat-picture", cx);
        });
        harness.with_window(cx, |window, cx| {
            assert!(
                window.find("chat-picture-viewer").visible(),
                "a click on the picture leaves it open"
            );
            window.press("escape", cx);
        });
        harness.with_window(cx, |window, _| {
            assert!(window.try_find("chat-picture-viewer").is_none());
        });
        assert_eq!(
            harness.status(cx),
            Status::Running,
            "Escape closed only the picture"
        );
    }

    #[gpui_kit::test]
    fn removing_a_picture_does_not_show_it(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        let image = Image::from_bytes(ImageFormat::Png, PNG.to_vec());
        paste(&harness, cx, ClipboardEntry::Image(image));
        harness.with_window(cx, |window, cx| window.hover("chat-attachment-0", cx));
        harness.with_window(cx, |window, cx| {
            let remove = window.find_by_label("Remove");
            let id = remove.path().last().cloned().expect("an id");
            window.click(id, cx);
        });
        harness.with_window(cx, |window, _| {
            assert!(window.try_find("chat-attachment-0").is_none(), "removed");
            assert!(window.try_find("chat-picture-viewer").is_none());
        });
    }
}
