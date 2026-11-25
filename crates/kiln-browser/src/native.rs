//! `WebUSB`, `WebHID` and Web Serial, which reach no device in a browser
//! without a window. The page's script calls Kiln instead, which asks the
//! user, opens the device and answers each call; the devices themselves
//! are someone else's to reach.

use serde::Deserialize;
use serde_json::{Value, json};

use crate::{Page, event::PageEvent};

pub(crate) const SOURCE: &str = include_str!("native.js");
pub(crate) const BINDING: &str = "kilnNative";

/// A call the page waits on.
#[derive(Clone, Debug, PartialEq)]
pub struct NativeCall {
    /// `usb`, `hid` or `serial`.
    pub kind: String,
    pub method: String,
    /// The device it is made on, unless it asks for one.
    pub device: Option<String>,
    pub args: Value,
    id: u64,
    /// The document's script, which goes on once answered.
    context: i64,
}

/// Why a call failed, as the page's `DOMException`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeError {
    pub name: String,
    pub message: String,
}

#[derive(Deserialize)]
struct Message {
    id: u64,
    kind: String,
    method: String,
    device: Option<String>,
    #[serde(default)]
    args: Value,
}

/// The call in a call of the binding, if it is this one's.
pub(crate) fn parse(params: &Value) -> Option<PageEvent> {
    if params["name"] != BINDING {
        return None;
    }
    let message: Message = serde_json::from_str(params["payload"].as_str()?).ok()?;
    Some(PageEvent::NativeCall(NativeCall {
        kind: message.kind,
        method: message.method,
        device: message.device,
        args: message.args,
        id: message.id,
        context: params["executionContextId"].as_i64()?,
    }))
}

impl Page {
    /// Lets the page go on with what `call` came to.
    pub fn answer_native(&self, call: &NativeCall, done: Result<Value, NativeError>) {
        let (error, value) = match done {
            Ok(value) => (Value::Null, value),
            Err(error) => (
                json!({"name": error.name, "message": error.message}),
                Value::Null,
            ),
        };
        let answer = json!({
            "expression": format!("kilnNativeReply({}, {error}, {value})", call.id),
            "contextId": call.context,
        });
        self.send("Runtime.evaluate", &answer);
    }

    /// Tells the page what happened to a device: data that came in, or it
    /// being plugged in or out.
    pub fn native_event(&self, kind: &str, device: &str, event: &Value) {
        let arguments = json!([kind, device, event]);
        let expression = format!("globalThis.kilnNativeEvent?.(...{arguments})");
        self.send("Runtime.evaluate", &json!({"expression": expression}));
    }
}

#[cfg(test)]
mod installed;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_is_read_with_its_document() {
        let payload = r#"{"id":4,"kind":"usb","method":"transferIn","device":"usb:1","args":{"endpointNumber":1}}"#;
        let call = json!({"name": BINDING, "payload": payload, "executionContextId": 9});
        let Some(PageEvent::NativeCall(call)) = parse(&call) else {
            panic!("no call");
        };
        assert_eq!(
            (call.kind.as_str(), call.method.as_str()),
            ("usb", "transferIn")
        );
        assert_eq!(call.device.as_deref(), Some("usb:1"));
        assert_eq!(call.args["endpointNumber"], 1);
        assert_eq!((call.id, call.context), (4, 9));
        let asking = json!({"name": BINDING, "payload": r#"{"id":1,"kind":"hid","method":"requestDevice","device":null}"#, "executionContextId": 1});
        let Some(PageEvent::NativeCall(asking)) = parse(&asking) else {
            panic!("no call");
        };
        assert_eq!((asking.device, asking.args), (None, Value::Null));
    }
}
