//! What a page's scripts and requests report.

use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Error,
    Warning,
    Info,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsoleEntry {
    pub level: Level,
    pub text: String,
    /// The script's URL and line, or the URL that failed.
    pub place: Option<String>,
}

/// A page can write any amount.
const MAX_TEXT: usize = 2000;

impl ConsoleEntry {
    fn new(level: Level, text: &str, place: Option<String>) -> Self {
        Self {
            level,
            text: text.chars().take(MAX_TEXT).collect(),
            place,
        }
    }
}

pub(crate) fn parse(method: &str, params: &Value) -> Option<ConsoleEntry> {
    match method {
        "Runtime.consoleAPICalled" => written(params),
        "Runtime.exceptionThrown" => Some(thrown(&params["exceptionDetails"])),
        "Log.entryAdded" => Some(logged(&params["entry"])),
        _ => None,
    }
}

fn written(params: &Value) -> Option<ConsoleEntry> {
    let level = match params["type"].as_str()? {
        "error" | "assert" => Level::Error,
        "warning" => Level::Warning,
        // Not messages.
        "clear" | "startGroup" | "endGroup" | "profile" | "profileEnd" => return None,
        _ => Level::Info,
    };
    let text: Vec<String> = params["args"].as_array()?.iter().map(value).collect();
    let place = frame(&params["stackTrace"]);
    Some(ConsoleEntry::new(level, &text.join(" "), place))
}

fn thrown(details: &Value) -> ConsoleEntry {
    let text = details["exception"]["description"].as_str();
    let text = text.or(details["text"].as_str()).unwrap_or_default();
    ConsoleEntry::new(Level::Error, text, frame(&details["stackTrace"]))
}

fn logged(entry: &Value) -> ConsoleEntry {
    let level = match entry["level"].as_str() {
        Some("error") => Level::Error,
        Some("warning") => Level::Warning,
        _ => Level::Info,
    };
    let place = entry["url"].as_str().map(str::to_owned);
    ConsoleEntry::new(level, entry["text"].as_str().unwrap_or_default(), place)
}

/// Where the topmost call is written; lines count from zero.
fn frame(stack: &Value) -> Option<String> {
    let call = &stack["callFrames"][0];
    let url = call["url"].as_str().filter(|url| !url.is_empty())?;
    let line = call["lineNumber"].as_u64()? + 1;
    Some(format!("{url}:{line}"))
}

fn value(argument: &Value) -> String {
    if let Some(text) = argument["value"].as_str() {
        return text.to_owned();
    }
    if !argument["value"].is_null() {
        return argument["value"].to_string();
    }
    let described = argument["description"].as_str();
    let properties = argument["preview"]["properties"].as_array();
    match (described, properties) {
        (Some(text), Some(_)) if argument["subtype"] == "error" => text.to_owned(),
        (_, Some(properties)) => preview(properties, argument["subtype"] == "array"),
        (Some(text), None) => text.to_owned(),
        (None, None) => argument["type"].as_str().unwrap_or_default().to_owned(),
    }
}

fn preview(properties: &[Value], array: bool) -> String {
    let parts: Vec<String> = properties
        .iter()
        .map(|property| {
            let value = property["value"].as_str().unwrap_or("…");
            match property["name"].as_str() {
                Some(name) if !array => format!("{name}: {value}"),
                _ => value.to_owned(),
            }
        })
        .collect();
    if array {
        format!("[{}]", parts.join(", "))
    } else {
        format!("{{{}}}", parts.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn messages_read_as_in_the_browser() {
        let call = json!({
            "type": "error",
            "args": [
                {"type": "string", "value": "bad"},
                {"type": "number", "value": 3},
                {"type": "object", "description": "Object", "preview": {"properties": [
                    {"name": "a", "value": "1"}, {"name": "b", "value": "Array(2)"}]}},
                {"type": "object", "subtype": "error", "description": "Error: e\n at x",
                    "preview": {"properties": []}},
            ],
            "stackTrace": {"callFrames": [{"url": "http://a/app.js", "lineNumber": 9}]},
        });
        let entry = parse("Runtime.consoleAPICalled", &call).unwrap();
        assert_eq!(entry.level, Level::Error);
        assert_eq!(entry.text, "bad 3 {a: 1, b: Array(2)} Error: e\n at x");
        assert_eq!(entry.place.as_deref(), Some("http://a/app.js:10"));
    }

    #[test]
    fn failures_of_scripts_and_requests_are_errors() {
        let thrown = json!({"exceptionDetails": {"text": "Uncaught",
            "exception": {"description": "TypeError: x"}}});
        let entry = parse("Runtime.exceptionThrown", &thrown).unwrap();
        assert_eq!((entry.level, &*entry.text), (Level::Error, "TypeError: x"));
        let failed = json!({"entry": {"level": "error", "text": "404", "url": "http://a/x"}});
        let entry = parse("Log.entryAdded", &failed).unwrap();
        assert_eq!(entry.place.as_deref(), Some("http://a/x"));
    }
}
