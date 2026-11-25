//! Asking the user before a tool runs, and the rules that answer for them.

use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct PermissionRequest {
    pub id: String,
    pub tool: String,
    pub input: Value,
    pub description: Option<String>,
    pub remember: Option<Remember>,
    /// Set when the agent asks the user questions rather than for
    /// permission; answered with [`Decision::Answer`].
    pub questions: Vec<Question>,
    /// The call that started the subagent asking; `None` for the main agent.
    pub subagent: Option<String>,
}

/// A question the agent asks, with options to choose from. The user may
/// also answer in their own words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Question {
    pub text: String,
    /// A short label for the question, e.g. "Color".
    pub header: String,
    pub options: Vec<QuestionOption>,
    /// Whether several options may be chosen.
    pub multi_select: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestionOption {
    pub label: String,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    Allow,
    /// Allow, and stop asking for calls like this one (see [`Remember`])
    /// for as long and as widely as `RuleScope` says.
    AllowAlways(RuleScope),
    /// Deny; `feedback` tells the agent what to do instead.
    Deny {
        feedback: Option<String>,
    },
    /// Answer the request's [`questions`](PermissionRequest::questions): one
    /// answer each, in order. Several choices are joined with ", ".
    Answer(Vec<String>),
}

/// What [`Decision::AllowAlways`] would remember, when the agent offers it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Remember {
    /// Permission rules for this kind of call, as the agent writes them,
    /// e.g. `Bash(npm test:*)`.
    Rules(Vec<String>),
    /// Accept file edits without asking for the rest of the session.
    AcceptEdits,
}

/// How long and how widely an "allow always" answer applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleScope {
    /// Until the agent process ends.
    Session,
    /// In this project, kept in the user's own project settings rather than
    /// the ones shared with the repository.
    Project,
    /// In every project.
    User,
}
