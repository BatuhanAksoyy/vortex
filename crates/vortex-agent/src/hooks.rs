//! Hooks: commands the agent runs on its own events, such as before a
//! tool call or when a turn ends, as its settings list them.

use serde_json::Value;

/// The hooks the agent would run, and the events they can be added for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Hooks {
    pub hooks: Vec<Hook>,
    /// Every event a hook can be added for, in the order they happen.
    pub events: Vec<HookEvent>,
    /// Why hooks cannot be changed here, when the user's organization
    /// manages them.
    pub locked: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HookEvent {
    pub name: String,
    /// One line on when it happens.
    pub summary: String,
    /// Whether its hooks can be narrowed with a matcher, e.g. to one tool.
    pub takes_matcher: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hook {
    pub event: String,
    /// Empty when the hook runs for every match.
    pub matcher: String,
    /// Where it comes from, as the agent words it, e.g. a plugin.
    pub source: String,
    /// What runs: a command, a prompt or a URL.
    pub text: String,
    /// The kind of hook, e.g. `command` or `http`.
    pub kind: String,
    /// Whether policy or the session's mode keeps it from running.
    pub disabled: bool,
    /// Set on a hook kept in a settings file that can be edited here.
    pub stored: Option<StoredHook>,
}

/// A hook as its settings file holds it, which names it for removal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredHook {
    pub scope: HookScope,
    pub matcher: String,
    pub config: Value,
}

/// The settings a hook is kept in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HookScope {
    /// The user's own settings for this project.
    #[default]
    Project,
    /// The project settings shared with the repository.
    SharedProject,
    /// The user's settings for every project.
    User,
}

impl HookScope {
    pub const ALL: [Self; 3] = [Self::Project, Self::SharedProject, Self::User];
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HookEdit {
    /// Runs `command` on `event`, for the matches of `matcher` if any.
    Add {
        scope: HookScope,
        event: String,
        matcher: String,
        command: String,
    },
    /// Changes a stored command hook in place, in the same settings file;
    /// what the form does not show, such as a timeout, is kept.
    Replace {
        event: String,
        matcher: String,
        command: String,
        target_event: String,
        target: StoredHook,
    },
    Remove {
        event: String,
        stored: StoredHook,
    },
}
