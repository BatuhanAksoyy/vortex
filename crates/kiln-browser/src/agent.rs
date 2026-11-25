//! What an agent does with a page: it reads the page as an outline, and acts
//! on the elements the outline names by their `ref`.

use std::collections::HashMap;

use anyhow::{Context as _, bail};
use serde_json::{Value, json};

use crate::{
    Page,
    input::{Button, Modifiers, MouseInput, MouseKind},
    keys::Key,
};

/// Roles an agent acts on; only these get a `ref`.
const ACTIONABLE: [&str; 17] = [
    "button",
    "link",
    "textbox",
    "searchbox",
    "combobox",
    "checkbox",
    "radio",
    "switch",
    "slider",
    "spinbutton",
    "tab",
    "menuitem",
    "menuitemcheckbox",
    "menuitemradio",
    "option",
    "listbox",
    "treeitem",
];
/// Roles that only hold others.
const SILENT: [&str; 6] = [
    "generic",
    "none",
    "presentation",
    "RootWebArea",
    "LineBreak",
    "InlineTextBox",
];
/// States worth telling, as the tree names them.
const STATES: [&str; 7] = [
    "level", "checked", "pressed", "selected", "expanded", "disabled", "required",
];
/// A long page is cut; the agent can ask about the rest by script.
const MAX_LINES: usize = 1500;
const MAX_NAME: usize = 200;
const MAX_VALUE: usize = 20_000;

/// Puts the element's text where the next typing replaces it.
const SELECT_TEXT: &str = "function () {
  this.focus();
  if (typeof this.select === 'function') { this.select(); return; }
  const range = document.createRange();
  range.selectNodeContents(this);
  getSelection().removeAllRanges();
  getSelection().addRange(range);
}";

const GONE: &str = "The element is gone from the page; take a new snapshot.";

impl Page {
    /// The page's accessibility tree, one element a line, indented as nested.
    pub async fn snapshot(&self) -> anyhow::Result<String> {
        let tree = self.call("Accessibility.getFullAXTree", json!({})).await?;
        Ok(outline(&tree["nodes"]))
    }

    /// Scrolls the element into view and clicks its middle.
    pub async fn click_node(&self, node: i64) -> anyhow::Result<()> {
        let at = json!({"backendNodeId": node});
        let scrolled = self.call("DOM.scrollIntoViewIfNeeded", at.clone()).await;
        scrolled.context(GONE)?;
        let quads = self.call("DOM.getContentQuads", at).await.context(GONE)?;
        let (x, y) = middle(&quads).context("The element takes no space; it may be hidden.")?;
        let at = |kind, button| MouseInput {
            kind,
            x,
            y,
            button,
            clicks: 1,
            modifiers: Modifiers::default(),
        };
        for input in [
            at(MouseKind::Moved, None),
            at(MouseKind::Pressed, Some(Button::Left)),
            at(MouseKind::Released, Some(Button::Left)),
        ] {
            self.call("Input.dispatchMouseEvent", input.params())
                .await?;
        }
        Ok(())
    }

    /// Replaces the text of a field with `text`, as typed.
    pub async fn fill_node(&self, node: i64, text: &str) -> anyhow::Result<()> {
        let resolve = json!({"backendNodeId": node});
        let resolved = self.call("DOM.resolveNode", resolve).await.context(GONE)?;
        let select = json!({
            "objectId": resolved["object"]["objectId"],
            "functionDeclaration": SELECT_TEXT,
        });
        self.call("Runtime.callFunctionOn", select).await?;
        if text.is_empty() {
            return self.press("backspace").await;
        }
        self.call("Input.insertText", json!({"text": text})).await?;
        Ok(())
    }

    /// Presses and lets go of a key, by GPUI's name for it or the character
    /// it types. Done once the page handled both, as are the others here,
    /// so what the agent does next sees their effect.
    pub async fn press(&self, name: &str) -> anyhow::Result<()> {
        let name = name.to_lowercase();
        let mut characters = name.chars();
        let single = characters.next().filter(|_| characters.next().is_none());
        let key = Key {
            text: single.map(String::from),
            name,
            modifiers: Modifiers::default(),
            repeated: false,
        };
        self.call("Input.dispatchKeyEvent", key.down()).await?;
        self.call("Input.dispatchKeyEvent", key.up()).await?;
        Ok(())
    }

    /// The value of a script run in the page, as JSON; a promise is awaited.
    pub async fn evaluate(&self, expression: &str) -> anyhow::Result<String> {
        let run = json!({
            "expression": expression,
            "awaitPromise": true,
            "returnByValue": true,
            "userGesture": true,
        });
        let ran = self.call("Runtime.evaluate", run).await?;
        if let Some(failure) = ran.get("exceptionDetails") {
            let thrown = failure["exception"]["description"].as_str();
            bail!(
                "{}",
                thrown.or(failure["text"].as_str()).unwrap_or("It failed")
            );
        }
        let value = &ran["result"]["value"];
        let text = match value {
            Value::String(text) => text.clone(),
            Value::Null if ran["result"]["type"] == "undefined" => "undefined".to_owned(),
            value => value.to_string(),
        };
        Ok(cut(&text, MAX_VALUE))
    }

    /// Whether the document finished loading.
    pub async fn loaded(&self) -> bool {
        let state = self.evaluate("document.readyState").await;
        state.is_ok_and(|state| state == "complete")
    }
}

fn middle(quads: &Value) -> Option<(f32, f32)> {
    let points: Vec<f32> = serde_json::from_value(quads["quads"][0].clone()).ok()?;
    let average = |from: usize| points.iter().skip(from).step_by(2).sum::<f32>() / 4.;
    (points.len() == 8).then(|| (average(0), average(1)))
}

fn cut(text: &str, most: usize) -> String {
    match text.char_indices().nth(most) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_owned(),
    }
}

fn outline(nodes: &Value) -> String {
    let nodes = nodes.as_array().map_or_default(Vec::as_slice);
    let by_id = nodes
        .iter()
        .filter_map(|node| Some((node["nodeId"].as_str()?, node)))
        .collect();
    let mut outline = Outline {
        by_id,
        lines: Vec::new(),
    };
    if let Some(root) = nodes.first() {
        outline.walk(root, 0, "");
    }
    if outline.lines.len() >= MAX_LINES {
        outline
            .lines
            .push("… the rest of the page is left out".to_owned());
    }
    outline.lines.join("\n")
}

struct Outline<'a> {
    by_id: HashMap<&'a str, &'a Value>,
    lines: Vec<String>,
}

impl<'a> Outline<'a> {
    fn walk(&mut self, node: &'a Value, depth: usize, above: &'a str) {
        if self.lines.len() >= MAX_LINES {
            return;
        }
        let (depth, above) = match line(node, above) {
            Some(line) => {
                self.lines.push(format!("{}{line}", "  ".repeat(depth)));
                (depth + 1, name(node))
            }
            None => (depth, above),
        };
        let children = node["childIds"].as_array().map_or_default(Vec::as_slice);
        for child in children {
            let child = child.as_str().and_then(|id| self.by_id.get(id).copied());
            if let Some(child) = child {
                self.walk(child, depth, above);
            }
        }
    }
}

fn name(node: &Value) -> &str {
    node["name"]["value"].as_str().unwrap_or_default().trim()
}

/// How the outline shows `node`, if at all; text its element already names
/// is left out.
fn line(node: &Value, above: &str) -> Option<String> {
    if node["ignored"] == true {
        return None;
    }
    let role = node["role"]["value"].as_str().unwrap_or_default();
    let name = name(node);
    if role == "StaticText" {
        let shown = !name.is_empty() && name != above;
        return shown.then(|| format!("- text: {}", cut(name, MAX_NAME)));
    }
    let actionable = ACTIONABLE.contains(&role);
    if !actionable && (SILENT.contains(&role) || name.is_empty()) {
        return None;
    }
    let mut parts = vec![format!("- {role}")];
    if !name.is_empty() {
        parts.push(format!("\"{}\"", cut(name, MAX_NAME)));
    }
    parts.extend(states(node));
    let value = node["value"]["value"]
        .as_str()
        .filter(|value| !value.is_empty());
    parts.extend(value.map(|value| format!("value=\"{}\"", cut(value, MAX_NAME))));
    let reference = node["backendDOMNodeId"].as_i64().filter(|_| actionable);
    parts.extend(reference.map(|reference| format!("[ref={reference}]")));
    Some(parts.join(" "))
}

fn states(node: &Value) -> impl Iterator<Item = String> + '_ {
    let properties = node["properties"].as_array().map_or_default(Vec::as_slice);
    properties.iter().filter_map(|property| {
        let name = property["name"].as_str()?;
        let shown = STATES.contains(&name);
        match &property["value"]["value"] {
            _ if !shown => None,
            Value::Bool(true) => Some(format!("[{name}]")),
            Value::Bool(false) | Value::Null => None,
            Value::String(text) if text == "false" => None,
            Value::String(text) => Some(format!("[{name}={text}]")),
            value => Some(format!("[{name}={value}]")),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, role: &str, name: &str, children: &[&str]) -> Value {
        json!({"nodeId": id, "role": {"value": role}, "name": {"value": name},
            "childIds": children, "backendDOMNodeId": id.parse::<i64>().unwrap() * 10})
    }

    #[test]
    fn controls_get_refs_and_their_own_text_is_not_repeated() {
        let mut heading = node("3", "heading", "Cart", &["4"]);
        heading["properties"] = json!([{"name": "level", "value": {"value": 2}}]);
        let nodes = json!([
            node("1", "RootWebArea", "Shop", &["2", "3", "5"]),
            node("2", "generic", "", &["6"]),
            heading,
            node("4", "StaticText", "Cart", &[]),
            node("5", "button", "Pay", &["7"]),
            node("6", "StaticText", "3 items", &[]),
            node("7", "StaticText", "Pay", &[]),
        ]);
        let expected = "- text: 3 items\n- heading \"Cart\" [level=2]\n- button \"Pay\" [ref=50]";
        assert_eq!(outline(&nodes), expected);
    }

    #[test]
    fn the_middle_of_an_element() {
        let quads = json!({"quads": [[10., 20., 30., 20., 30., 60., 10., 60.]]});
        assert_eq!(middle(&quads), Some((20., 40.)));
        assert_eq!(middle(&json!({"quads": []})), None);
    }
}
