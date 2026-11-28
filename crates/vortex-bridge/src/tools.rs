//! The tools agents get, and what they hand back.

use serde_json::{Value, json};

use crate::settings_tools::{self, SettingsCall};

/// What an agent asks of Vortex.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    Browse(Browse),
    /// Commit `paths`, or every change when empty, with `message`; in
    /// `folder` when the agent works outside its project folder.
    Commit {
        message: String,
        paths: Vec<String>,
        folder: Option<String>,
    },
    Settings(SettingsCall),
}

/// What an agent asks of the browser beside its chat.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Browse {
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
    /// The device to pair or connect to, by id or name.
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
    site Vortex asks the user first, and the call waits for the answer.";

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
    tools.push(commit_tool());
    tools.extend(settings_tools::list());
    Value::Array(tools)
}

/// For answering what the page asks the user.
fn question_tools() -> [Value; 2] {
    [
        tool(
            "browser_questions",
            "What the page waits on the user for: a dialog (alert, confirm, prompt, or leaving the page), the chooser for a Bluetooth, USB, HID or serial device with the devices found, or a request to use the camera, microphone, location, notifications or MIDI. A call that sets one off returns as soon as it is asked.",
            &json!({}),
            &[],
        ),
        tool(
            "browser_answer",
            "Answers a question from browser_questions for the user. `accept` false dismisses a dialog, cancels the chooser, or blocks the request. A prompt takes `text`; a chooser takes `device`, an id or a name from the list. A permission answered here holds for that request only.",
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

fn commit_tool() -> Value {
    tool(
        "commit",
        "Commits changes in the project's repository with `message`; use it instead of `git commit`. Without `paths` every change is committed, new files too; with them only those files, relative to the project folder. Working in another checkout of the repository, such as a worktree, give its absolute path as `folder`; `paths` are then relative to it. Write the message as the project's history does: a short first line, a blank line, then why. Do not credit yourself or any tool in it. Vortex takes such lines out, checks the message against the project's rules and may show it to the user to approve or edit first; the call waits for that and says what was committed, or why not.",
        &json!({
            "message": {"type": "string"},
            "paths": {"type": "array", "items": {"type": "string"}},
            "folder": {"type": "string"},
        }),
        &["message"],
    )
}

pub(crate) fn tool(name: &str, description: &str, properties: &Value, required: &[&str]) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": {"type": "object", "properties": properties, "required": required},
    })
}

/// The call `name` makes with `arguments`, or what is wrong with it.
pub(crate) fn parse(name: &str, arguments: &Value) -> Result<Request, String> {
    if name == "commit" {
        return commit(arguments);
    }
    if let Some(call) = settings_tools::parse(name, arguments) {
        return call;
    }
    browse(name, arguments).map(Request::Browse)
}

fn browse(name: &str, arguments: &Value) -> Result<Browse, String> {
    let text = |key: &str| {
        let value = arguments[key].as_str().map(str::to_owned);
        value.ok_or_else(|| format!("`{key}` must be a string"))
    };
    let target = || {
        let value = arguments["ref"].as_i64();
        value.ok_or_else(|| "`ref` must be a number from browser_snapshot".to_owned())
    };
    Ok(match name {
        "browser_navigate" => Browse::Navigate { url: text("url")? },
        "browser_snapshot" => Browse::Snapshot,
        "browser_click" => Browse::Click { target: target()? },
        "browser_type" => Browse::Type {
            target: target()?,
            text: text("text")?,
            submit: arguments["submit"].as_bool().unwrap_or(false),
        },
        "browser_press_key" => Browse::PressKey { key: text("key")? },
        "browser_screenshot" => Browse::Screenshot,
        "browser_console" => Browse::Console,
        "browser_evaluate" => Browse::Evaluate {
            expression: text("expression")?,
        },
        _ => return question_call(name, arguments),
    })
}

fn question_call(name: &str, arguments: &Value) -> Result<Browse, String> {
    match name {
        "browser_questions" => Ok(Browse::Questions),
        "browser_answer" => answer(arguments).map(Browse::Answer),
        _ => Err(format!("There is no tool named {name}")),
    }
}

fn commit(arguments: &Value) -> Result<Request, String> {
    let message = arguments["message"].as_str();
    let message = message.ok_or_else(|| "`message` must be a string".to_owned())?;
    let paths = match &arguments["paths"] {
        Value::Null => Vec::new(),
        Value::Array(paths) => paths
            .iter()
            .map(|path| path.as_str().map(str::to_owned))
            .collect::<Option<_>>()
            .ok_or_else(|| "`paths` must be a list of strings".to_owned())?,
        _ => return Err("`paths` must be a list of strings".to_owned()),
    };
    let folder = match &arguments["folder"] {
        Value::Null => None,
        Value::String(folder) => Some(folder.clone()),
        _ => return Err("`folder` must be a string".to_owned()),
    };
    Ok(Request::Commit {
        message: message.to_owned(),
        paths,
        folder,
    })
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
            "question": "device", "accept": true, "message": "x", "value": 1,
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
        let expected = Browse::Type {
            target: 3,
            text: "hi".into(),
            submit: false,
        };
        assert_eq!(typed, Ok(Request::Browse(expected)));
        let paired = parse(
            "browser_answer",
            &json!({"question": "device", "accept": true, "device": "Vortex Heart"}),
        );
        let Ok(Request::Browse(Browse::Answer(paired))) = paired else {
            panic!("not an answer");
        };
        assert_eq!(
            (paired.question, paired.device.as_deref()),
            (Question::Device, Some("Vortex Heart"))
        );
        let wrong = parse(
            "browser_answer",
            &json!({"question": "file", "accept": true}),
        )
        .unwrap_err();
        assert!(wrong.contains("`question`"), "{wrong}");
        let committed = parse("commit", &json!({"message": "Fix", "paths": ["a.rs"]}));
        let expected = Request::Commit {
            message: "Fix".into(),
            paths: vec!["a.rs".into()],
            folder: None,
        };
        assert_eq!(committed, Ok(expected));
        let elsewhere = parse("commit", &json!({"message": "Fix", "folder": "/w/one"}));
        let expected = Request::Commit {
            message: "Fix".into(),
            paths: Vec::new(),
            folder: Some("/w/one".into()),
        };
        assert_eq!(elsewhere, Ok(expected));
        let wrong = parse("commit", &json!({"message": "Fix", "folder": 3})).unwrap_err();
        assert!(wrong.contains("`folder`"), "{wrong}");
        let wrong = parse("commit", &json!({"message": "Fix", "paths": "a.rs"})).unwrap_err();
        assert!(wrong.contains("`paths`"), "{wrong}");
        let wrong = parse("browser_click", &json!({"ref": "e3"})).unwrap_err();
        assert!(wrong.contains("`ref`"), "{wrong}");
    }
}
