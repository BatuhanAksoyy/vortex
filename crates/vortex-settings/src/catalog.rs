//! The settings a person or an agent changes one at a time: each one's key,
//! what it is called and does, and the values it takes. The settings page
//! is drawn from it, and the agents' settings tools describe and check
//! against it. Lists and rules stay in the settings file and the commit
//! settings page.

use serde_json::Value;

use crate::{edit::get, load::DEFAULT_SETTINGS, load::parse_jsonc, merge::merge, model::Settings};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Appearance,
    Editor,
    Files,
    Layout,
    Chat,
    Git,
    Updates,
}

impl Section {
    pub const ALL: [Self; 7] = [
        Self::Appearance,
        Self::Editor,
        Self::Files,
        Self::Layout,
        Self::Chat,
        Self::Git,
        Self::Updates,
    ];

    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Editor => "Editor",
            Self::Files => "Files",
            Self::Layout => "Layout",
            Self::Chat => "Chat",
            Self::Git => "Git",
            Self::Updates => "Updates",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Toggle,
    Number {
        min: f64,
        max: f64,
        step: f64,
    },
    Text,
    /// One of these values, each with its label.
    Choice(&'static [(&'static str, &'static str)]),
    /// The name of a theme Vortex knows.
    Theme,
}

#[derive(Clone, Copy, Debug)]
pub struct Entry {
    /// Dotted, such as `editor.font_size`.
    pub key: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub kind: Kind,
    pub section: Section,
}

impl Entry {
    /// The path to the value, as [`crate::get`] and [`crate::Layers`] take it.
    #[must_use]
    pub fn path(&self) -> Vec<&'static str> {
        self.key.split('.').collect()
    }
}

const fn entry(
    section: Section,
    key: &'static str,
    title: &'static str,
    description: &'static str,
    kind: Kind,
) -> Entry {
    Entry {
        key,
        title,
        description,
        kind,
        section,
    }
}

const FONT: Kind = Kind::Number {
    min: 6.,
    max: 72.,
    step: 1.,
};
const SIDES: &[(&str, &str)] = &[("left", "Left"), ("right", "Right")];

use Kind::{Choice, Text, Theme, Toggle};
use Section::{Appearance, Chat, Editor, Files, Git, Layout, Updates};

pub const CATALOG: &[Entry] = &[
    entry(
        Appearance,
        "theme.mode",
        "Color theme",
        "Follow the system, or always use the light or the dark theme.",
        Choice(&[("system", "System"), ("light", "Light"), ("dark", "Dark")]),
    ),
    entry(
        Appearance,
        "theme.light",
        "Light theme",
        "The theme used in light mode.",
        Theme,
    ),
    entry(
        Appearance,
        "theme.dark",
        "Dark theme",
        "The theme used in dark mode.",
        Theme,
    ),
    entry(
        Appearance,
        "ui.font_size",
        "Interface font size",
        "Every part of the interface scales with it.",
        FONT,
    ),
    entry(
        Appearance,
        "ui.font_family",
        "Interface font",
        "Empty uses the platform's font.",
        Text,
    ),
    entry(
        Editor,
        "editor.font_size",
        "Font size",
        "The size of code in the editor, diffs and tool output.",
        FONT,
    ),
    entry(
        Editor,
        "editor.font_family",
        "Font",
        "Empty uses the platform's monospace font.",
        Text,
    ),
    entry(
        Editor,
        "editor.tab_size",
        "Tab size",
        "Columns a tab takes.",
        Kind::Number {
            min: 1.,
            max: 16.,
            step: 1.,
        },
    ),
    entry(Editor, "editor.line_numbers", "Line numbers", "", Toggle),
    entry(Editor, "editor.soft_wrap", "Wrap long lines", "", Toggle),
    entry(Editor, "editor.indent_guides", "Indent guides", "", Toggle),
    entry(
        Editor,
        "editor.minimap.enabled",
        "Minimap",
        "The outline of the whole file beside the editor.",
        Toggle,
    ),
    entry(
        Editor,
        "editor.gutter.folds",
        "Folding",
        "Blocks of code fold from the gutter.",
        Toggle,
    ),
    entry(
        Editor,
        "editor.typing.auto_close_brackets",
        "Close brackets",
        "Typing an opening bracket or quote adds its closing one.",
        Toggle,
    ),
    entry(
        Editor,
        "editor.typing.auto_indent",
        "Keep indentation",
        "A new line keeps the indentation of the one before it.",
        Toggle,
    ),
    entry(
        Editor,
        "editor.show_whitespace",
        "Show whitespace",
        "Spaces and tabs are drawn as faint marks.",
        Choice(&[("none", "None"), ("all", "All")]),
    ),
    entry(
        Editor,
        "editor.scroll_beyond_last_line",
        "Scroll past the last line",
        "",
        Choice(&[("half_page", "Half a page"), ("off", "Off")]),
    ),
    entry(
        Files,
        "files.auto_save",
        "Auto save",
        "Save an edited file once typing stops.",
        Choice(&[("off", "Off"), ("after_delay", "After a delay")]),
    ),
    entry(
        Files,
        "files.auto_save_delay",
        "Auto save delay",
        "Milliseconds without typing before the file is saved.",
        Kind::Number {
            min: 100.,
            max: 60_000.,
            step: 100.,
        },
    ),
    entry(
        Layout,
        "layout.side_panel",
        "Side panel",
        "Where the activity bar and the Files, Search and Changes panel sit.",
        Choice(SIDES),
    ),
    entry(
        Layout,
        "layout.browser",
        "Browser",
        "Where the browser panel opens beside the chats.",
        Choice(&[("right", "Right"), ("left", "Left"), ("bottom", "Bottom")]),
    ),
    entry(
        Layout,
        "layout.file_tree",
        "File tree",
        "Where the tree sits on the Files page.",
        Choice(SIDES),
    ),
    entry(
        Layout,
        "layout.activity_bar",
        "Activity bar",
        "Hidden, its pages stay on their commands and keys.",
        Toggle,
    ),
    entry(Layout, "layout.status_bar", "Status bar", "", Toggle),
    entry(
        Layout,
        "layout.title_bar_buttons",
        "Title bar buttons",
        "The buttons that show the side panel and the browser.",
        Toggle,
    ),
    entry(
        Chat,
        "chat.sleep_after_minutes",
        "Sleep after",
        "Minutes an idle chat's agent keeps running; it starts again with the next message. 0 keeps it running.",
        Kind::Number {
            min: 0.,
            max: 1440.,
            step: 5.,
        },
    ),
    entry(
        Chat,
        "chat.usage_meter",
        "Usage meter",
        "Which usage window the chat's meter shows.",
        Choice(&[
            ("fuller", "Nearest limit"),
            ("session", "Session"),
            ("weekly", "Weekly"),
            ("both", "Both"),
        ]),
    ),
    entry(
        Chat,
        "chat.colors.claude",
        "Claude Code color",
        "As #rrggbb; empty keeps its own. The chat's pencil button offers some.",
        Text,
    ),
    entry(
        Chat,
        "chat.colors.codex",
        "Codex color",
        "As #rrggbb; empty keeps its own.",
        Text,
    ),
    entry(
        Chat,
        "agents.claude_path",
        "Claude Code program",
        "The claude program to run, as a path. Empty looks on PATH and where \
         its installers put it.",
        Text,
    ),
    entry(
        Chat,
        "agents.codex_path",
        "Codex program",
        "The codex program to run, as a path. Empty looks on PATH and where \
         its installers put it.",
        Text,
    ),
    entry(
        Git,
        "git.commit.enabled",
        "Commit a chat's work",
        "A strip over the chat counts the files it changed and commits them.",
        Toggle,
    ),
    entry(
        Git,
        "git.commit.review",
        "Review each commit",
        "A commit waits for you to approve or edit its message.",
        Toggle,
    ),
    entry(
        Git,
        "git.push.policy",
        "Push",
        "When Vortex pushes a branch.",
        Choice(&[
            ("never", "Never"),
            ("manual", "With the Push button"),
            ("after_commit", "After each commit"),
            ("session_end", "When the chat closes"),
        ]),
    ),
    entry(
        Updates,
        "update.check",
        "Check for updates",
        "Look for a newer Vortex on GitHub at start and every few hours.",
        Toggle,
    ),
];

/// The entry for dotted `key`.
#[must_use]
pub fn find(key: &str) -> Option<&'static Entry> {
    CATALOG.iter().find(|entry| entry.key == key)
}

/// Why `value` cannot be the setting at dotted `key`, checked against
/// the whole settings so the type and the range both hold.
///
/// # Errors
/// When there is no such setting, or the value does not fit it.
pub fn check(key: &str, value: &Value) -> anyhow::Result<()> {
    let mut merged = parse_jsonc(DEFAULT_SETTINGS)?;
    let path: Vec<&str> = key.split('.').collect();
    if path.iter().any(|name| name.is_empty()) || get(&merged, &path).is_none() {
        anyhow::bail!("there is no setting `{key}`");
    }
    let mut layer = Value::Null;
    crate::set_in_value(&mut layer, &path, Some(value.clone()));
    merge(&mut merged, layer);
    let settings: Settings = serde_json::from_value(merged)
        .map_err(|error| anyhow::anyhow!("`{key}` cannot be {value}: {error}"))?;
    let errors = settings.range_errors();
    if !errors.is_empty() {
        anyhow::bail!(errors.join("; "));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
