//! What the browser reports about a page.

use base64::Engine as _;
use serde_json::Value;

/// A picture of the page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub jpeg: Vec<u8>,
    /// Names the frame when asking for the next one.
    pub(crate) ack: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PageEvent {
    Frame(Frame),
    /// Another document is shown. Frames must be asked for again.
    Opened,
    /// The document's place or title may be another one.
    Navigated,
    Loading(bool),
    /// The page wants a window of its own, for this URL.
    Popup(String),
    /// The page waits for an answer to `alert`, `confirm` or the like.
    Dialog {
        message: String,
        leaving: bool,
    },
    Crashed,
}

/// The event in a message about the page in frame `main`, if Kiln follows it.
pub(crate) fn parse(method: &str, params: &Value, main: &str) -> Option<PageEvent> {
    let text = |key: &str| params[key].as_str().unwrap_or_default().to_owned();
    let of_main = || params["frameId"] == main;
    match method {
        "Page.screencastFrame" => frame(params),
        "Page.frameNavigated" => params["frame"]["parentId"]
            .is_null()
            .then_some(PageEvent::Opened),
        "Page.navigatedWithinDocument" => of_main().then_some(PageEvent::Navigated),
        // The title is known by then.
        "Page.loadEventFired" | "Page.domContentEventFired" => Some(PageEvent::Navigated),
        "Page.frameStartedLoading" => of_main().then_some(PageEvent::Loading(true)),
        "Page.frameStoppedLoading" => of_main().then_some(PageEvent::Loading(false)),
        "Page.windowOpen" => Some(PageEvent::Popup(text("url"))),
        "Page.javascriptDialogOpening" => Some(PageEvent::Dialog {
            message: text("message"),
            leaving: params["type"] == "beforeunload",
        }),
        "Inspector.targetCrashed" => Some(PageEvent::Crashed),
        _ => None,
    }
}

fn frame(params: &Value) -> Option<PageEvent> {
    let data = params["data"].as_str()?;
    let jpeg = base64::engine::general_purpose::STANDARD
        .decode(data)
        .ok()?;
    let ack = params["sessionId"].as_u64()?;
    Some(PageEvent::Frame(Frame { jpeg, ack }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_the_pages_own_frame_counts() {
        let loading = |id: &str| parse("Page.frameStartedLoading", &json!({"frameId": id}), "main");
        assert_eq!(loading("main"), Some(PageEvent::Loading(true)));
        assert_eq!(loading("advert"), None);
        let inner = json!({"frame": {"id": "x", "parentId": "main"}});
        assert_eq!(parse("Page.frameNavigated", &inner, "main"), None);
        let outer = json!({"frame": {"id": "main"}});
        assert_eq!(
            parse("Page.frameNavigated", &outer, "main"),
            Some(PageEvent::Opened)
        );
    }

    #[test]
    fn frames_carry_their_picture() {
        let params = json!({"data": "aGk=", "sessionId": 7});
        let expected = Frame {
            jpeg: b"hi".to_vec(),
            ack: 7,
        };
        assert_eq!(
            parse("Page.screencastFrame", &params, "main"),
            Some(PageEvent::Frame(expected))
        );
        let damaged = json!({"data": "?", "sessionId": 7});
        assert_eq!(parse("Page.screencastFrame", &damaged, "main"), None);
    }
}
