//! Picking an element of the page, and pictures of the page.

use anyhow::Context as _;
use base64::Engine as _;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::Page;

const DESCRIBE: &str = include_str!("element.js");
/// Larger pictures are sent as JPEG; agents take up to 5 MB.
const MAX_PNG: usize = 4 * 1024 * 1024;
const JPEG_QUALITY: u8 = 80;

/// What the page says about one of its elements.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct Element {
    /// E.g. `button.primary`.
    pub name: String,
    pub selector: String,
    pub text: String,
    pub html: String,
    pub width: i64,
    pub height: i64,
    /// The component that made it and where that is written, if the page's
    /// framework tells.
    pub component: Option<String>,
    pub source: Option<String>,
}

/// A picture of the page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Screenshot {
    pub media_type: &'static str,
    pub data: Vec<u8>,
}

impl Page {
    /// Highlights what is under the pointer until an element is clicked,
    /// which `PageEvent::Picked` names, or this is called off.
    pub fn pick(&self, on: bool) {
        let touch = self.is_phone() && !on;
        // Elements are picked by the mouse only.
        let touch = json!({"enabled": touch});
        self.send("Emulation.setEmitTouchEventsForMouse", &touch);
        let mode = if on { "searchForNode" } else { "none" };
        let color = |r: u8, g: u8, b: u8| json!({"r": r, "g": g, "b": b, "a": 0.35});
        let inspect = json!({
            "mode": mode,
            "highlightConfig": {
                "showInfo": true,
                "contentColor": color(111, 168, 220),
                "paddingColor": color(147, 196, 125),
                "marginColor": color(246, 178, 107),
            },
        });
        self.send("Overlay.setInspectMode", &inspect);
        if !on {
            self.send("Overlay.hideHighlight", &json!({}));
        }
    }

    pub async fn element(&self, node: i64) -> anyhow::Result<Element> {
        let resolve = json!({"backendNodeId": node});
        let resolved = self.call("DOM.resolveNode", resolve).await?;
        let describe = json!({
            "objectId": resolved["object"]["objectId"],
            "functionDeclaration": DESCRIBE,
            "returnByValue": true,
        });
        let described = self.call("Runtime.callFunctionOn", describe).await?;
        let text = described["result"]["value"].as_str();
        let text = text.context("the page says nothing about the element")?;
        Ok(serde_json::from_str(text)?)
    }

    /// The part of the page in view.
    pub async fn screenshot(&self) -> anyhow::Result<Screenshot> {
        self.capture(None).await
    }

    /// The element at `node`, as far as it is in view.
    pub async fn element_screenshot(&self, node: i64) -> anyhow::Result<Screenshot> {
        let at = json!({"backendNodeId": node});
        let quads = self.call("DOM.getContentQuads", at).await?;
        let metrics = self.call("Page.getLayoutMetrics", json!({})).await?;
        let clip = clip(&quads, &metrics["cssVisualViewport"]);
        self.capture(Some(clip.context("the element is not in view")?))
            .await
    }

    async fn capture(&self, clip: Option<Value>) -> anyhow::Result<Screenshot> {
        let png = self.capture_as("png", clip.clone()).await?;
        if png.len() <= MAX_PNG {
            return Ok(Screenshot {
                media_type: "image/png",
                data: png,
            });
        }
        Ok(Screenshot {
            media_type: "image/jpeg",
            data: self.capture_as("jpeg", clip).await?,
        })
    }

    async fn capture_as(&self, format: &str, clip: Option<Value>) -> anyhow::Result<Vec<u8>> {
        let mut capture = json!({"format": format});
        if format == "jpeg" {
            capture["quality"] = JPEG_QUALITY.into();
        }
        if let Some(clip) = clip {
            capture["clip"] = clip;
        }
        let captured = self.call("Page.captureScreenshot", capture).await?;
        let data = captured["data"].as_str().context("no picture came")?;
        Ok(base64::engine::general_purpose::STANDARD.decode(data)?)
    }
}

/// The part of the document the element's first box takes up in view.
fn clip(quads: &Value, view: &Value) -> Option<Value> {
    let quad = quads["quads"][0].as_array()?;
    let side = |at: usize| quad.iter().skip(at).step_by(2).filter_map(Value::as_f64);
    let least = |at| side(at).fold(f64::INFINITY, f64::min).max(0.);
    let most = |at, end: f64| side(at).fold(0., f64::max).min(end);
    let (left, top) = (least(0), least(1));
    let right = most(0, view["clientWidth"].as_f64()?);
    let bottom = most(1, view["clientHeight"].as_f64()?);
    if right - left < 1. || bottom - top < 1. {
        return None;
    }
    Some(json!({
        "x": left + view["pageX"].as_f64()?,
        "y": top + view["pageY"].as_f64()?,
        "width": right - left,
        "height": bottom - top,
        "scale": 1,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_element_is_cut_to_what_is_in_view() {
        let view = json!({"clientWidth": 400, "clientHeight": 300, "pageX": 0, "pageY": 1000});
        let quads = json!({"quads": [[-10., 250., 500., 250., 500., 350., -10., 350.]]});
        let cut = clip(&quads, &view).unwrap();
        assert_eq!(cut["x"], 0.0);
        assert_eq!(cut["y"], 1250.0);
        assert_eq!(cut["width"], 400.0);
        assert_eq!(cut["height"], 50.0);
        let below = json!({"quads": [[0., 310., 10., 310., 10., 320., 0., 320.]]});
        assert_eq!(clip(&below, &view), None);
    }
}
