//! What the script in each document reports: the parts of a page the browser
//! leaves to the system, which frames do not show.

use serde::Deserialize;
use serde_json::Value;

use crate::event::PageEvent;

pub(crate) const SOURCE: &str = include_str!("guest.js");
pub(crate) const WORLD: &str = "kiln";
pub(crate) const BINDING: &str = "kilnSend";

/// A `<select>` was opened.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Select {
    pub choices: Vec<Choice>,
    /// The chosen one's index, or a negative number.
    pub selected: i64,
    /// Whether the place counts from where the pointer was pressed, rather
    /// than from the page's top left.
    pub from_pointer: bool,
    /// Under the closed select.
    pub x: f32,
    pub y: f32,
    pub width: f32,
    /// The document's script, which takes the choice.
    #[serde(skip)]
    pub(crate) context: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Choice {
    pub index: i64,
    pub label: String,
    pub group: Option<String>,
    pub disabled: bool,
}

/// What a context menu was asked for on.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct MenuTarget {
    pub link: Option<String>,
    pub image: Option<String>,
    pub editable: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum Message {
    Cursor(String),
    Select(Select),
    Menu(MenuTarget),
}

/// The event in a call of the script's binding.
pub(crate) fn parse(params: &Value) -> Option<PageEvent> {
    if params["name"] != BINDING {
        return None;
    }
    let message = serde_json::from_str(params["payload"].as_str()?).ok()?;
    Some(match message {
        Message::Cursor(cursor) => PageEvent::Cursor(cursor),
        Message::Menu(target) => PageEvent::ContextMenu(target),
        Message::Select(select) => PageEvent::Select(Select {
            context: params["executionContextId"].as_i64()?,
            ..select
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_select_keeps_the_document_it_is_in() {
        let payload = r#"{"select":{"choices":[{"index":0,"label":"A","group":null,
            "disabled":false,"hidden":false}],"selected":0,"fromPointer":true,
            "x":-3,"y":9.5,"width":80}}"#;
        let call = json!({"name": BINDING, "payload": payload, "executionContextId": 5});
        let Some(PageEvent::Select(select)) = parse(&call) else {
            panic!("no select");
        };
        assert_eq!((select.context, select.y), (5, 9.5));
        assert_eq!(select.choices[0].label, "A");
        let other = json!({"name": "theirs", "payload": payload, "executionContextId": 5});
        assert_eq!(parse(&other), None);
    }
}
