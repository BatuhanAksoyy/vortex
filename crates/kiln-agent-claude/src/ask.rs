//! Questions the agent asks with its `AskUserQuestion` tool. They arrive as a
//! permission request for that tool; the answers go back in the allowed
//! call's input, under `answers`, keyed by each question's text.

use kiln_agent::{Question, QuestionOption};
use serde::Deserialize;
use serde_json::{Map, Value};

const TOOL: &str = "AskUserQuestion";

#[derive(Deserialize)]
struct Input {
    questions: Vec<WireQuestion>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireQuestion {
    question: String,
    #[serde(default)]
    header: String,
    #[serde(default)]
    options: Vec<WireOption>,
    #[serde(default)]
    multi_select: bool,
}

#[derive(Deserialize)]
struct WireOption {
    label: String,
    #[serde(default)]
    description: String,
}

/// The questions a call to `tool` asks. Empty when it is another tool, or
/// when the input cannot be read: the call is then shown as an ordinary
/// permission request rather than lost.
pub fn questions(tool: &str, input: &Value) -> Vec<Question> {
    if tool != TOOL {
        return Vec::new();
    }
    let Ok(input) = Input::deserialize(input) else {
        log::warn!("unreadable {TOOL} input: {input}");
        return Vec::new();
    };
    input
        .questions
        .into_iter()
        .map(|question| Question {
            text: question.question,
            header: question.header,
            options: question
                .options
                .into_iter()
                .map(|option| QuestionOption {
                    label: option.label,
                    description: option.description,
                })
                .collect(),
            multi_select: question.multi_select,
        })
        .collect()
}

/// The call's input with `answers` added, keyed by each question's text as
/// the agent wrote it.
pub fn answered_input(input: &Value, answers: &[String]) -> Value {
    let texts = input
        .get("questions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|question| question.get("question").and_then(Value::as_str));
    let answers: Map<String, Value> = texts
        .zip(answers)
        .map(|(text, answer)| (text.to_owned(), Value::String(answer.clone())))
        .collect();
    let mut input = input.clone();
    if let Some(fields) = input.as_object_mut() {
        fields.insert("answers".to_owned(), Value::Object(answers));
    }
    input
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn other_tools_and_unreadable_input_ask_nothing() {
        let input = json!({ "questions": [{ "question": "Q?", "options": [] }] });
        assert_eq!(questions("Bash", &input).len(), 0);
        assert_eq!(questions(TOOL, &json!({ "questions": 3 })).len(), 0);
        assert_eq!(questions(TOOL, &input).len(), 1);
    }

    #[test]
    fn answers_are_keyed_by_question_text() {
        let input = json!({ "questions": [{ "question": "A?" }, { "question": "B?" }] });
        let answered = answered_input(&input, &["x".into(), "y, z".into()]);
        assert_eq!(answered["answers"], json!({ "A?": "x", "B?": "y, z" }));
        assert_eq!(answered["questions"], input["questions"]);
    }
}
