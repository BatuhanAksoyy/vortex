//! How tool calls read in the chat: a short argument for the row header, a
//! one-line result summary, and for file edits the removed and added lines.

use std::path::Path;

use serde_json::Value;

/// Input fields worth showing in a row header, most specific first.
const DETAIL_KEYS: &[&str] = &[
    "command",
    "file_path",
    "notebook_path",
    "path",
    "pattern",
    "url",
    "query",
    "description",
    "prompt",
    // SendMessage: what the message asks.
    "summary",
    // Typing into a page, pressing a key and running a script there.
    "text",
    "key",
    "expression",
];

/// How a tool is named in its row: MCP tools, which the CLI calls
/// `mcp__<server>__<tool>`, by their own words, as `Browser Click`.
pub fn title(tool: &str) -> String {
    if tool == "advisor" {
        return "Advisor".to_owned();
    }
    let Some(name) = tool
        .strip_prefix("mcp__")
        .and_then(|rest| rest.split_once("__"))
    else {
        return tool.to_owned();
    };
    let words = name.1.split(['_', '-']).filter(|word| !word.is_empty());
    let words: Vec<String> = words
        .map(|word| {
            let mut letters = word.chars();
            letters.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(letters).collect()
            })
        })
        .collect();
    words.join(" ")
}

/// The argument shown after the tool name, e.g. `npm test` for Bash. Paths
/// inside `root` are shown relative to it.
pub fn detail(input: &Value, root: &Path) -> String {
    let value = DETAIL_KEYS
        .iter()
        .find_map(|key| input.get(*key).and_then(Value::as_str))
        .unwrap_or_default();
    let first_line = value.lines().next().unwrap_or_default();
    Path::new(first_line).strip_prefix(root).map_or_else(
        |_| first_line.to_owned(),
        |relative| relative.display().to_string(),
    )
}

/// The file name a file tool works on, for prompts like "Make this edit to
/// `main.rs`?".
pub fn file_name(input: &Value) -> Option<&str> {
    Path::new(file_path(input)?).file_name()?.to_str()
}

/// The file a file tool such as Read or Edit works on.
pub fn file_path(input: &Value) -> Option<&str> {
    input
        .get("file_path")
        .or_else(|| input.get("notebook_path"))?
        .as_str()
}

/// The text after `└` under a finished tool row.
pub fn summary(tool: &str, output: &str) -> String {
    let count = output.lines().count();
    match tool {
        "Read" => format!("Read {}", plural(count, "line")),
        "Glob" => format!("Found {}", plural(count, "file")),
        // The CLI answers with JSON meant for the model.
        "SendMessage" if output.starts_with(r#"{"success":true"#) => "Sent".to_owned(),
        // The CLI gets the advice encrypted, for the model alone.
        "advisor" if output.trim().is_empty() => {
            "Advised the model; the advice is not shown".to_owned()
        }
        _ => output
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or("No output")
            .to_owned(),
    }
}

/// Tools whose output is shown in full, like a terminal.
pub fn shows_output(tool: &str) -> bool {
    tool == "Bash"
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct EditDiff {
    pub removed: Vec<String>,
    pub added: Vec<String>,
    /// A Write replaces the whole file, so its lines are numbered from 1.
    pub whole_file: bool,
}

impl EditDiff {
    pub fn summary(&self) -> String {
        match (self.added.len(), self.removed.len()) {
            (added, 0) if self.whole_file => format!("Wrote {}", plural(added, "line")),
            (added, 0) => format!("Added {}", plural(added, "line")),
            (0, removed) => format!("Removed {}", plural(removed, "line")),
            (added, removed) => format!(
                "Added {}, removed {}",
                plural(added, "line"),
                plural(removed, "line")
            ),
        }
    }
}

/// Lines an Edit, `MultiEdit` or Write call removes and adds. Not a minimal
/// diff: the tool input already names exactly the replaced text.
pub fn edit_diff(tool: &str, input: &Value) -> Option<EditDiff> {
    let mut diff = EditDiff::default();
    match tool {
        "Edit" => push_edit(&mut diff, input),
        "MultiEdit" => input
            .get("edits")?
            .as_array()?
            .iter()
            .for_each(|edit| push_edit(&mut diff, edit)),
        "Write" => {
            diff.added = lines(input.get("content"));
            diff.whole_file = true;
        }
        _ => return None,
    }
    Some(diff)
}

fn push_edit(diff: &mut EditDiff, edit: &Value) {
    diff.removed.extend(lines(edit.get("old_string")));
    diff.added.extend(lines(edit.get("new_string")));
}

fn lines(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_str)
        .map_or_else(Vec::new, |text| text.lines().map(str::to_owned).collect())
}

fn plural(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn mcp_tools_are_named_by_their_own_words() {
        assert_eq!(title("mcp__vortex__browser_press_key"), "Browser Press Key");
        assert_eq!(title("mcp__github__list-issues"), "List Issues");
        assert_eq!(title("Bash"), "Bash");
        assert_eq!(title("mcp__odd"), "mcp__odd");
    }

    #[test]
    fn detail_picks_the_meaningful_argument() {
        let root = Path::new("/project");
        assert_eq!(
            detail(&json!({ "command": "npm test\nmore" }), root),
            "npm test"
        );
        assert_eq!(
            detail(&json!({ "file_path": "/project/src/main.rs" }), root),
            "src/main.rs"
        );
        assert_eq!(
            detail(&json!({ "file_path": "/elsewhere/a.rs" }), root),
            "/elsewhere/a.rs"
        );
        assert_eq!(detail(&json!({ "todos": [] }), root), "");
    }

    #[test]
    fn summaries_count_or_quote_the_first_line() {
        assert_eq!(summary("Read", "1\ta\n2\tb"), "Read 2 lines");
        assert_eq!(summary("Glob", "a.rs"), "Found 1 file");
        assert_eq!(summary("Grep", "\nFound 3 files\nx"), "Found 3 files");
        assert_eq!(summary("Task", ""), "No output");
        assert_eq!(
            summary("advisor", ""),
            "Advised the model; the advice is not shown"
        );
        assert_eq!(summary("advisor", "Take two numbers."), "Take two numbers.");
        assert_eq!(title("advisor"), "Advisor");
        assert_eq!(
            summary(
                "SendMessage",
                r#"{"success":true,"message":"Resuming agent a84f85d"}"#
            ),
            "Sent"
        );
    }

    #[test]
    fn edit_diffs_cover_edit_multi_edit_and_write() {
        let edit = edit_diff("Edit", &json!({ "old_string": "a\nb", "new_string": "c" })).unwrap();
        assert_eq!(edit.removed, ["a", "b"]);
        assert_eq!(edit.summary(), "Added 1 line, removed 2 lines");
        let multi = json!({ "edits": [{ "old_string": "x", "new_string": "y" }, { "old_string": "", "new_string": "z" }] });
        assert_eq!(edit_diff("MultiEdit", &multi).unwrap().added, ["y", "z"]);
        let write = edit_diff("Write", &json!({ "content": "1\n2" })).unwrap();
        assert_eq!(
            (write.whole_file, write.summary()),
            (true, "Wrote 2 lines".to_owned())
        );
        assert_eq!(edit_diff("Bash", &json!({})), None);
    }

    #[test]
    fn file_name_is_the_last_component() {
        assert_eq!(
            file_name(&json!({ "file_path": "/p/src/utils.py" })),
            Some("utils.py")
        );
        assert_eq!(file_name(&json!({ "command": "ls" })), None);
    }
}
