//! Questions a page asks and waits on.

use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogKind {
    Alert,
    Confirm,
    /// Asks for a line of text.
    Prompt,
    /// The page has something to lose when it is left.
    Leave,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dialog {
    pub kind: DialogKind,
    pub message: String,
    /// What a prompt suggests.
    pub suggested: String,
    /// The document asking.
    pub url: String,
}

pub(crate) fn parse(params: &Value) -> Dialog {
    let text = |key: &str| params[key].as_str().unwrap_or_default().to_owned();
    let kind = match params["type"].as_str() {
        Some("confirm") => DialogKind::Confirm,
        Some("prompt") => DialogKind::Prompt,
        Some("beforeunload") => DialogKind::Leave,
        _ => DialogKind::Alert,
    };
    Dialog {
        kind,
        message: text("message"),
        suggested: text("defaultPrompt"),
        url: text("url"),
    }
}
