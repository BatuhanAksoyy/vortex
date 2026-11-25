//! The tools agents get, and what they hand back.

use serde_json::{Value, json};

/// What an agent asks of the browser beside its chat.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    Navigate {
        url: String,
    },
    Snapshot,
    /// `target` is a `ref` the last snapshot gave.
    Click {
        target: i64,
    },
    Type {
        target: i64,
        text: String,
        submit: bool,
    },
    PressKey {
        key: String,
    },
    Screenshot,
    Console,
    Evaluate {
        expression: String,
    },
    /// What the page waits on the user for.
    Questions,
    Answer(Answer),
}

/// The agent's answer to a question the page asks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Answer {
    pub question: Question,
    pub accept: bool,
    /// A prompt's text.
    pub text: Option<String>,
    /// The Bluetooth device to pair, by id or name.
    pub device: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Question {
    Dialog,
    Device,
    Permission,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Content {
    Text(String),
    Image { media_type: String, data: Vec<u8> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reply {
    pub content: Vec<Content>,
    pub error: bool,
}

impl Reply {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![Content::Text(text.into())],
            error: false,
        }
    }

    pub fn failed(text: impl Into<String>) -> Self {
        Self {
            error: true,
            ..Self::text(text)
        }
    }
}

const SITES: &str = "Pages on this machine (localhost) open freely; for any other \
    site Kiln asks the user first, and the call waits for the answer.";

pub(crate) fn list() -> Value {
    let target = json!({"type": "integer", "description": "The element's `ref` from the latest browser_snapshot"});
    let mut tools = vec![
        tool(
            "browser_navigate",
            &format!(
                "Opens a URL in the browser panel beside the chat, which the user sees, and waits until it has loaded. {SITES}"
            ),
            &json!({"url": {"type": "string", "description": "e.g. http://localhost:3000/login"}}),
            &["url"],
        ),
        tool(
            "browser_snapshot",
            "The open page as an outline of its accessibility tree: headings, text, and the controls, each with a `ref` to click or type into. Use it to read the page and before acting on it.",
            &json!({}),
            &[],
        ),
        tool(
            "browser_click",
            "Clicks an element of the page, as the user would with the mouse.",
            &json!({"ref": target}),
            &["ref"],
        ),
        tool(
            "browser_type",
            "Replaces the text in a field of the page with `text`, as if typed, and presses Enter after it when `submit` is true.",
            &json!({"ref": target, "text": {"type": "string"}, "submit": {"type": "boolean"}}),
            &["ref", "text"],
        ),
        tool(
            "browser_press_key",
            "Presses a key on the page: a character, or enter, tab, escape, backspace, delete, up, down, left, right, home, end, pageup, pagedown, space.",
            &json!({"key": {"type": "string"}}),
            &["key"],
        ),
        tool(
            "browser_screenshot",
            "A picture of the part of the page in view.",
            &json!({}),
            &[],
        ),
        tool(
            "browser_console",
            "What the page's scripts and failed requests wrote to the console since it loaded, newest last.",
            &json!({}),
            &[],
        ),
        tool(
            "browser_evaluate",
            "Runs a JavaScript expression in the page and returns its value as JSON; a promise is awaited. For measuring and checking what the outline does not show, such as computed styles or sizes.",
            &json!({"expression": {"type": "string"}}),
            &["expression"],
        ),
    ];
    tools.extend(question_tools());
    Value::Array(tools)
}

/// For answering what the page asks the user.
fn question_tools() -> [Value; 2] {
    [
        tool(
            "browser_questions",
            "What the page waits on the user for: a dialog (alert, confirm, prompt, or leaving the page), the Bluetooth device chooser with the devices found so far, or a request to use the camera, microphone, location, notifications or MIDI. A call that sets one off returns as soon as it is asked.",
            &json!({}),
            &[],
        ),
        tool(
            "browser_answer",
            "Answers a question from browser_questions for the user. `accept` false dismisses a dialog, cancels the chooser, or blocks the request. A prompt takes `text`; pairing takes `device`, an id or a name from the list. A permission answered here holds for that request only.",
            &json!({
                "question": {"type": "string", "enum": ["dialog", "device", "permission"]},
                "accept": {"type": "boolean"},
                "text": {"type": "string"},
                "device": {"type": "string"},
            }),
            &["question", "accept"],
        ),
    ]
}

fn tool(name: &str, description: &str, properties: &Value, required: &[&str]) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": {"type": "object", "properties": properties, "required": required},
    })
}

/// The call `name` makes with `arguments`, or what is wrong with it.
pub(crate) fn parse(name: &str, arguments: &Value) -> Result<Request, String> {
    let text = |key: &str| {
        let value = arguments[key].as_str().map(str::to_owned);
        value.ok_or_else(|| format!("`{key}` must be a string"))
    };
    let target = || {
        let value = arguments["ref"].as_i64();
        value.ok_or_else(|| "`ref` must be a number from browser_snapshot".to_owned())
    };
    Ok(match name {
        "browser_navigate" => Request::Navigate { url: text("url")? },
        "browser_snapshot" => Request::Snapshot,
        "browser_click" => Request::Click { target: target()? },
        "browser_type" => Request::Type {
            target: target()?,
            text: text("text")?,
            submit: arguments["submit"].as_bool().unwrap_or(false),
        },
        "browser_press_key" => Request::PressKey { key: text("key")? },
        "browser_screenshot" => Request::Screenshot,
        "browser_console" => Request::Console,
        "browser_evaluate" => Request::Evaluate {
            expression: text("expression")?,
        },
        _ => return question_call(name, arguments),
    })
}

fn question_call(name: &str, arguments: &Value) -> Result<Request, String> {
    match name {
        "browser_questions" => Ok(Request::Questions),
        "browser_answer" => answer(arguments).map(Request::Answer),
        _ => Err(format!("There is no tool named {name}")),
    }
}

fn answer(arguments: &Value) -> Result<Answer, String> {
    let question = match arguments["question"].as_str() {
        Some("dialog") => Question::Dialog,
        Some("device") => Question::Device,
        Some("permission") => Question::Permission,
        _ => return Err("`question` must be dialog, device or permission".to_owned()),
    };
    let accept = arguments["accept"].as_bool();
    let accept = accept.ok_or_else(|| "`accept` must be true or false".to_owned())?;
    let optional = |key: &str| arguments[key].as_str().map(str::to_owned);
    Ok(Answer {
        question,
        accept,
        text: optional("text"),
        device: optional("device"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_tool_can_be_called() {
        let arguments = json!({
            "url": "x", "ref": 1, "text": "x", "key": "x", "expression": "x",
            "question": "device", "accept": true,
        });
        let tools = list();
        for tool in tools.as_array().unwrap() {
            let name = tool["name"].as_str().unwrap();
            assert!(parse(name, &arguments).is_ok(), "{name}");
        }
    }

    #[test]
    fn wrong_arguments_are_explained() {
        let typed = parse("browser_type", &json!({"ref": 3, "text": "hi"}));
        let expected = Request::Type {
            target: 3,
            text: "hi".into(),
            submit: false,
        };
        assert_eq!(typed, Ok(expected));
        let paired = parse(
            "browser_answer",
            &json!({"question": "device", "accept": true, "device": "Kiln Heart"}),
        );
        let Ok(Request::Answer(paired)) = paired else {
            panic!("not an answer");
        };
        assert_eq!(
            (paired.question, paired.device.as_deref()),
            (Question::Device, Some("Kiln Heart"))
        );
        let wrong = parse(
            "browser_answer",
            &json!({"question": "file", "accept": true}),
        )
        .unwrap_err();
        assert!(wrong.contains("`question`"), "{wrong}");
        let wrong = parse("browser_click", &json!({"ref": "e3"})).unwrap_err();
        assert!(wrong.contains("`ref`"), "{wrong}");
    }
}
