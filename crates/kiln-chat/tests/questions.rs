//! The agent's questions, answered in the chat in place of the composer.

// Inside a `cfg(test)` module so helpers may `expect`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod questions {
    use std::time::Duration;

    use gpui_kit::TestAppContext;
    use gpui_kit::test::TestWindowExt;
    use kiln_agent::{
        AgentEvent, Decision, PermissionRequest, Question, QuestionOption, SessionCommand,
    };
    use kiln_chat::Status;
    use serde_json::json;

    use super::common::Harness;

    fn question(text: &str, multi_select: bool, labels: &[&str]) -> Question {
        Question {
            text: text.into(),
            header: "Pick".into(),
            options: labels
                .iter()
                .map(|label| QuestionOption {
                    label: (*label).into(),
                    description: format!("Choose {label}"),
                })
                .collect(),
            multi_select,
        }
    }

    /// A chat, focused and mid-turn, that the agent asks `questions`.
    fn asked(cx: &mut TestAppContext, questions: Vec<Question>) -> Harness {
        let harness = Harness::new(cx);
        harness.type_and_send(cx, "help me choose");
        harness.commands();
        harness.emit(
            cx,
            AgentEvent::PermissionRequest(PermissionRequest {
                id: "ask-1".into(),
                tool: "AskUserQuestion".into(),
                input: json!({}),
                description: None,
                remember: None,
                questions,
            }),
        );
        assert!(shown(&harness, cx, "question-prompt"));
        harness
    }

    fn color(cx: &mut TestAppContext) -> Harness {
        asked(cx, vec![question("Which color?", false, &["Red", "Blue"])])
    }

    fn press(harness: &Harness, cx: &mut TestAppContext, key: &str) {
        harness.with_window(cx, |window, cx| window.press(key, cx));
    }

    /// Lets a picked single answer show before it is confirmed.
    fn settle(cx: &mut TestAppContext) {
        cx.executor().advance_clock(Duration::from_millis(500));
        cx.run_until_parked();
    }

    fn shown(harness: &Harness, cx: &mut TestAppContext, id: &'static str) -> bool {
        harness.with_window(cx, |window, _| window.try_find(id).is_some())
    }

    fn answered(answers: &[&str]) -> SessionCommand {
        SessionCommand::Permission {
            request_id: "ask-1".into(),
            decision: Decision::Answer(answers.iter().map(|&a| a.to_owned()).collect()),
        }
    }

    fn click(harness: &Harness, cx: &mut TestAppContext, id: &'static str) {
        harness.with_window(cx, |window, cx| window.click(id, cx));
    }

    #[gpui_kit::test]
    fn a_picked_option_is_sent_with_submit(cx: &mut TestAppContext) {
        let harness = color(cx);
        press(&harness, cx, "1");
        assert_eq!(harness.commands(), Vec::new(), "nothing is answered yet");
        press(&harness, cx, "down");
        press(&harness, cx, "enter");
        settle(cx);
        assert_eq!(harness.commands(), Vec::new(), "picking does not send");
        click(&harness, cx, "question-submit");
        assert_eq!(harness.commands(), [answered(&["Blue"])]);
        assert!(!shown(&harness, cx, "question-prompt"));
        assert!(shown(&harness, cx, "chat-composer"));
        assert_eq!(harness.status(cx), Status::Running, "the turn goes on");
    }

    #[gpui_kit::test]
    fn every_question_is_answered_before_anything_is_sent(cx: &mut TestAppContext) {
        let harness = asked(
            cx,
            vec![
                question("Which fruits?", true, &["Apple", "Banana", "Cherry"]),
                question("Which city?", false, &["Paris", "Rome"]),
            ],
        );
        click(&harness, cx, "question-option-0");
        click(&harness, cx, "question-option-2");
        press(&harness, cx, "1");
        assert_eq!(harness.commands(), Vec::new(), "one question is left");
        click(&harness, cx, "question-tab-1");
        click(&harness, cx, "question-option-1");
        settle(cx);
        press(&harness, cx, "1");
        assert_eq!(harness.commands(), [answered(&["Apple, Cherry", "Rome"])]);
    }

    #[gpui_kit::test]
    fn a_single_choice_moves_on_to_the_next_question(cx: &mut TestAppContext) {
        let harness = asked(
            cx,
            vec![
                question("Which city?", false, &["Paris", "Rome"]),
                question("Which fruits?", true, &["Apple", "Banana"]),
            ],
        );
        press(&harness, cx, "enter");
        settle(cx);
        press(&harness, cx, "space");
        press(&harness, cx, "1");
        assert_eq!(harness.commands(), [answered(&["Paris", "Apple"])]);
    }

    #[gpui_kit::test]
    fn own_words_answer_and_digits_in_them_stay_text(cx: &mut TestAppContext) {
        let harness = color(cx);
        click(&harness, cx, "question-option-2");
        harness.with_window(cx, |window, cx| window.input("Navy 1", cx));
        assert_eq!(harness.commands(), Vec::new(), "typing answers nothing yet");
        click(&harness, cx, "question-submit");
        assert_eq!(harness.commands(), [answered(&["Navy 1"])]);
    }

    #[gpui_kit::test]
    fn escape_leaves_own_words_before_declining(cx: &mut TestAppContext) {
        let harness = color(cx);
        click(&harness, cx, "question-option-2");
        harness.with_window(cx, |window, cx| window.input("Na", cx));
        press(&harness, cx, "escape");
        assert_eq!(harness.commands(), Vec::new(), "the draft is kept");
        assert!(shown(&harness, cx, "question-prompt"));
        press(&harness, cx, "escape");
        assert_eq!(harness.commands(), [declined()]);
    }

    #[gpui_kit::test]
    fn close_declines_to_answer(cx: &mut TestAppContext) {
        let harness = color(cx);
        click(&harness, cx, "question-close");
        assert_eq!(harness.commands(), [declined()]);
        assert!(!shown(&harness, cx, "question-prompt"));
        let status = cx.update(|cx| harness.view.read(cx).status());
        assert_eq!(status, Status::Running);
    }

    fn declined() -> SessionCommand {
        SessionCommand::Permission {
            request_id: "ask-1".into(),
            decision: Decision::Deny {
                feedback: Some("I'd rather not answer these questions.".into()),
            },
        }
    }
}
