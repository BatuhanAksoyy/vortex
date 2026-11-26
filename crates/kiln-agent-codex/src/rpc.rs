//! The app-server's JSON-RPC framing: one JSON object per line, without the
//! `jsonrpc` member. Messages Kiln sends are built here; messages it reads
//! are split into responses, requests and notifications.

use serde_json::{Value, json};

#[derive(Debug, PartialEq)]
pub enum Incoming {
    /// The answer to a request Kiln sent; `Err` holds the error's message.
    Response {
        id: i64,
        result: Result<Value, String>,
    },
    /// The server asks Kiln something, such as an approval, and waits for a
    /// response under `id`.
    Request {
        id: Value,
        method: String,
        params: Value,
    },
    Notification {
        method: String,
        params: Value,
    },
}

impl Incoming {
    /// `None` for a line that is no JSON-RPC message.
    pub fn parse(line: &str) -> Option<Self> {
        let mut message: Value = serde_json::from_str(line).ok()?;
        let params = message.get_mut("params").map_or_default(Value::take);
        let method = message
            .get("method")
            .and_then(Value::as_str)
            .map(str::to_owned);
        match (method, message.get("id")) {
            (Some(method), Some(id)) => Some(Self::Request {
                id: id.clone(),
                method,
                params,
            }),
            (Some(method), None) => Some(Self::Notification { method, params }),
            (None, Some(id)) => Some(Self::Response {
                id: id.as_i64()?,
                result: response_result(&mut message),
            }),
            (None, None) => None,
        }
    }
}

fn response_result(message: &mut Value) -> Result<Value, String> {
    if let Some(error) = message.get("error") {
        let text = error.get("message").and_then(Value::as_str);
        return Err(text.unwrap_or("the request failed").to_owned());
    }
    Ok(message.get_mut("result").map_or_default(Value::take))
}

pub fn request(id: i64, method: &str, params: Value) -> Value {
    object([
        ("id", id.into()),
        ("method", method.into()),
        ("params", params),
    ])
}

pub fn notification(method: &str) -> Value {
    json!({ "method": method })
}

pub fn response(id: Value, result: Value) -> Value {
    object([("id", id), ("result", result)])
}

/// Turns down a server request Kiln does not handle.
pub fn refusal(id: Value, method: &str) -> Value {
    let error = json!({ "code": -32601, "message": format!("Kiln does not handle {method}") });
    object([("id", id), ("error", error)])
}

fn object<const N: usize>(members: [(&str, Value); N]) -> Value {
    let members = members
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value));
    Value::Object(members.collect())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::Incoming;

    #[test]
    fn splits_responses_requests_and_notifications() {
        let parse = |value: serde_json::Value| Incoming::parse(&value.to_string());
        assert_eq!(
            parse(json!({"id": 2, "result": {"ok": true}})),
            Some(Incoming::Response {
                id: 2,
                result: Ok(json!({"ok": true}))
            })
        );
        assert_eq!(
            parse(json!({"id": 4, "error": {"code": -32600, "message": "no"}})),
            Some(Incoming::Response {
                id: 4,
                result: Err("no".to_owned())
            })
        );
        assert_eq!(
            parse(json!({"id": 0, "method": "item/fileChange/requestApproval", "params": {}})),
            Some(Incoming::Request {
                id: json!(0),
                method: "item/fileChange/requestApproval".to_owned(),
                params: json!({})
            })
        );
        assert_eq!(
            parse(json!({"method": "turn/started", "params": {"a": 1}})),
            Some(Incoming::Notification {
                method: "turn/started".to_owned(),
                params: json!({"a": 1})
            })
        );
        assert_eq!(Incoming::parse("not json"), None);
    }
}
