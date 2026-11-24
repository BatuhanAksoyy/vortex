//! Reopening a stored conversation: its messages come back and the next
//! process resumes it.

// Inside a `cfg(test)` module so helpers may `expect`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod history {
    use gpui_kit::TestAppContext;
    use gpui_kit::test::{TestQueryExt, TestWindowExt};
    use kiln_agent::{AgentEvent, BlockKey, PastMessage, TurnOutcome};
    use kiln_chat::{ChatEvent, Status};

    use super::common::Harness;

    fn stored() -> Vec<PastMessage> {
        vec![
            PastMessage::User("What is in note.txt?".into()),
            PastMessage::Agent(AgentEvent::TextDelta {
                block: BlockKey {
                    message_id: "m-1".into(),
                    index: 0,
                },
                text: "A lantern.".into(),
            }),
        ]
    }

    fn shown(harness: &Harness, cx: &mut TestAppContext, label: &'static str) -> bool {
        harness.with_window(cx, move |window, _| {
            window.try_find_by_label(label).is_some()
        })
    }

    fn open(harness: &Harness, cx: &mut TestAppContext, id: &'static str) {
        open_as(harness, cx, id, false);
    }

    fn open_as(harness: &Harness, cx: &mut TestAppContext, id: &'static str, elsewhere: bool) {
        let view = harness.view.clone();
        harness.with_window(cx, move |window, cx| {
            view.update(cx, |view, cx| view.open_session(id, elsewhere, window, cx));
        });
        cx.run_until_parked();
    }

    fn with_stored(cx: &mut TestAppContext) -> Harness {
        let harness = Harness::new(cx);
        harness.script.borrow_mut().stored = vec![("s-9".into(), stored())];
        harness
    }

    #[gpui_kit::test]
    fn a_stored_conversation_opens_with_its_messages_and_resumes(cx: &mut TestAppContext) {
        let harness = with_stored(cx);
        harness.type_and_send(cx, "the current one");
        harness.emit(cx, AgentEvent::TurnEnded(TurnOutcome::Completed));
        let spawned = harness.script.borrow().spawned;
        open(&harness, cx, "s-9");
        assert!(shown(&harness, cx, "What is in note.txt?"));
        assert!(!shown(&harness, cx, "the current one"));
        let script = harness.script.borrow();
        assert_eq!(script.spawned, spawned + 1, "a process for it");
        let spec = script.last_spec.as_ref().expect("spawned");
        assert_eq!(spec.resume.as_deref(), Some("s-9"));
        drop(script);
        let id = cx.update(|cx| harness.view.read(cx).session_id().map(str::to_owned));
        assert_eq!(id.as_deref(), Some("s-9"));
        assert_eq!(harness.status(cx), Status::Idle);
        assert_eq!(harness.commands(), Vec::new(), "nothing is sent for it");
    }

    #[gpui_kit::test]
    fn opening_is_announced_for_the_history_list(cx: &mut TestAppContext) {
        let harness = with_stored(cx);
        let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let seen = events.clone();
        let view = harness.view.clone();
        cx.update(|cx| {
            cx.subscribe(&view, move |_, event: &ChatEvent, _| {
                seen.borrow_mut().push(*event);
            })
            .detach();
        });
        open(&harness, cx, "s-9");
        assert_eq!(*events.borrow(), [ChatEvent::SessionChanged]);
    }

    #[gpui_kit::test]
    fn a_running_turn_is_not_cut_off_by_opening_another(cx: &mut TestAppContext) {
        let harness = with_stored(cx);
        harness.type_and_send(cx, "the current one");
        open(&harness, cx, "s-9");
        assert!(shown(&harness, cx, "the current one"));
        assert_eq!(harness.status(cx), Status::Running);
    }

    #[gpui_kit::test]
    fn a_conversation_that_cannot_be_read_says_so(cx: &mut TestAppContext) {
        let harness = with_stored(cx);
        open(&harness, cx, "s-gone");
        assert!(shown(
            &harness,
            cx,
            "Cannot open the conversation: no stored conversation"
        ));
    }

    const COPY_NOTICE: &str = "Open in another window. Messages sent here continue a copy of it.";

    #[gpui_kit::test]
    fn a_conversation_open_elsewhere_continues_as_a_copy(cx: &mut TestAppContext) {
        let harness = with_stored(cx);
        open_as(&harness, cx, "s-9", true);
        assert!(shown(&harness, cx, "What is in note.txt?"));
        assert!(shown(&harness, cx, COPY_NOTICE));
        let fork = |harness: &Harness| {
            let script = harness.script.borrow();
            let spec = script.last_spec.as_ref().expect("spawned");
            (spec.resume.clone(), spec.fork)
        };
        assert_eq!(fork(&harness), (Some("s-9".into()), true));
        harness.type_and_send(cx, "go on");
        harness.emit(
            cx,
            AgentEvent::Started {
                session_id: "s-copy".into(),
                model: "claude-opus-5-5".into(),
            },
        );
        let id = cx.update(|cx| harness.view.read(cx).session_id().map(str::to_owned));
        assert_eq!(
            id.as_deref(),
            Some("s-copy"),
            "the copy is followed from here"
        );
        harness.emit(cx, AgentEvent::Exited { error: None });
        harness.type_and_send(cx, "and again");
        assert_eq!(
            fork(&harness),
            (Some("s-copy".into()), false),
            "the copy resumes itself"
        );
    }

    fn composer_in_panel(harness: &Harness, cx: &mut TestAppContext) -> bool {
        harness.with_window(cx, |window, _| {
            window.find("chat-composer").bounds().bottom() <= window.find("chat").bounds().bottom()
        })
    }

    #[gpui_kit::test]
    fn a_long_message_shows_its_first_lines_until_expanded(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        let lines: Vec<String> = (1..=60).map(|n| format!("line {n}")).collect();
        let text = lines.join("\n");
        let stored = vec![PastMessage::User(text.clone().into())];
        harness.script.borrow_mut().stored = vec![("s-long".into(), stored)];
        open(&harness, cx, "s-long");
        assert!(
            composer_in_panel(&harness, cx),
            "a title line, not the message"
        );
        let height = |cx: &mut TestAppContext| {
            harness.with_window(cx, |window, _| {
                window.find_by_label(&text).bounds().size.height
            })
        };
        let collapsed = height(cx);
        assert!(shown(&harness, cx, "Show all 60 lines"));
        harness.with_window(cx, |window, cx| {
            let more = window.find_by_label("Show all 60 lines");
            let id = more.path().last().cloned().expect("an id");
            window.click(id, cx);
        });
        assert!(shown(&harness, cx, "Show less"));
        assert!(height(cx) > collapsed, "expanded");
        assert!(composer_in_panel(&harness, cx));
    }

    #[gpui_kit::test]
    fn a_long_conversation_leaves_the_composer_in_the_panel(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        let stored = (1..=80)
            .map(|n| PastMessage::User(format!("message {n}").into()))
            .collect();
        harness.script.borrow_mut().stored = vec![("s-long".into(), stored)];
        open(&harness, cx, "s-long");
        let (composer, panel) = harness.with_window(cx, |window, _| {
            (
                window.find("chat-composer").bounds(),
                window.find("chat").bounds(),
            )
        });
        assert!(
            composer.bottom() <= panel.bottom(),
            "{composer:?} outside {panel:?}"
        );
    }

    fn id_number(id: &gpui_kit::ElementId) -> u64 {
        match id {
            gpui_kit::ElementId::NamedInteger(_, number) => *number,
            other => panic!("not a numbered id: {other:?}"),
        }
    }

    #[gpui_kit::test]
    fn a_message_widens_again_with_its_panel(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        let text = "a message long enough to wrap when the panel is narrowed ".repeat(4);
        let stored = vec![PastMessage::User(text.trim_end().to_owned().into())];
        harness.script.borrow_mut().stored = vec![("s-wide".into(), stored)];
        open(&harness, cx, "s-wide");
        let width = |cx: &mut TestAppContext| {
            harness.with_window(cx, |window, _| {
                let row = window.find_by_label(text.trim_end());
                let id = row.path().last().cloned().expect("an id");
                let bubble = format!("user-bubble-{}", id_number(&id));
                window
                    .find(gpui_kit::ElementId::Name(bubble.into()))
                    .bounds()
                    .size
                    .width
            })
        };
        let wide = width(cx);
        let size = cx.update(|cx| {
            harness
                .window
                .update(cx, |_, window, _| window.viewport_size())
                .expect("window")
        });
        cx.simulate_window_resize(harness.window, gpui_kit::size(size.width / 3., size.height));
        assert!(width(cx) < wide, "narrowed");
        cx.simulate_window_resize(harness.window, size);
        width(cx);
        assert_eq!(width(cx), wide, "as wide as before");
    }
}
