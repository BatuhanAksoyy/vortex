//! The canned agent state the preview shows: context, commands, hooks and
//! skills.

use kiln_agent::{
    AgentEvent, ContextCategory, ContextKind, ContextUsage, Effort, Hook, HookEdit, HookEvent,
    HookScope, Hooks, SessionCommand, Skill, SkillLock, SkillState, SlashCommand, StoredHook,
};
use serde_json::json;

/// The skills, hooks and effort the preview's panels list and change.
pub struct Settings {
    skills: Vec<Skill>,
    hooks: Hooks,
    effort: Effort,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            skills: preview_skills(),
            hooks: preview_hooks(),
            effort: Effort::High,
        }
    }
}

impl Settings {
    /// Answers the panels' requests as the CLI does; other commands get no
    /// reply.
    pub fn reply(&mut self, command: SessionCommand) -> Vec<AgentEvent> {
        match command {
            SessionCommand::SetSkillState { name, state } => {
                set_skill_state(&mut self.skills, &name, state);
            }
            SessionCommand::EditHook(edit) => {
                edit_hooks(&mut self.hooks.hooks, edit);
                return vec![
                    AgentEvent::HookEdited { error: None },
                    AgentEvent::Hooks(self.hooks.clone()),
                ];
            }
            SessionCommand::SetEffort(effort) => {
                self.effort = effort;
                return vec![AgentEvent::EffortChanged(Some(effort))];
            }
            SessionCommand::RefreshEffort => {
                return vec![AgentEvent::EffortChanged(Some(self.effort))];
            }
            SessionCommand::RefreshSkills | SessionCommand::RefreshHooks => {}
            _ => return Vec::new(),
        }
        vec![
            AgentEvent::Skills(self.skills.clone()),
            AgentEvent::Hooks(self.hooks.clone()),
        ]
    }
}

/// Context as the CLI breaks it down, deferred tools included.
pub fn context() -> ContextUsage {
    let category = |name: &str, tokens: u64, kind: ContextKind| ContextCategory {
        name: name.into(),
        tokens,
        kind,
    };
    ContextUsage {
        used_tokens: 41_200,
        max_tokens: 200_000,
        percent: Some(21),
        categories: vec![
            category("System prompt", 3_400, ContextKind::Used),
            category("System tools", 2_400, ContextKind::Used),
            category("System tools (deferred)", 19_200, ContextKind::Deferred),
            category("Skills", 7_500, ContextKind::Used),
            category("Messages", 27_900, ContextKind::Used),
            category("Autocompact buffer", 33_000, ContextKind::Buffer),
            category("Free space", 125_800, ContextKind::Free),
        ],
    }
}

/// Built-in commands as the CLI lists them, long hints included.
pub fn preview_commands() -> Vec<SlashCommand> {
    [
        ("clear", "[name]", "Start a new session with empty context"),
        (
            "compact",
            "<optional custom summarization instructions>",
            "Free up context by summarizing the conversation so far",
        ),
        ("context", "", "Show current context usage"),
        (
            "release-notes",
            "",
            "Draft release notes from the merged changes",
        ),
        (
            "docs:style-guide",
            "",
            "Check prose against the house style",
        ),
        (
            "effort",
            "<low|medium|high|xhigh|max|auto>",
            "Set effort level for model usage",
        ),
        (
            "code-review",
            "[low|medium|high|xhigh|max|ultra] [--fix] [--comment]",
            "Review the current diff for correctness bugs",
        ),
    ]
    .into_iter()
    .map(|(name, hint, description)| SlashCommand {
        name: name.into(),
        description: description.into(),
        argument_hint: hint.into(),
        aliases: Vec::new(),
    })
    .collect()
}

fn edit_hooks(hooks: &mut Vec<Hook>, edit: HookEdit) {
    match edit {
        HookEdit::Add {
            scope,
            event,
            matcher,
            command,
        } => hooks.push(hook(&event, &matcher, &command, scope)),
        HookEdit::Replace {
            event,
            matcher,
            command,
            target_event,
            target,
        } => {
            hooks
                .retain(|hook| hook.event != target_event || hook.stored.as_ref() != Some(&target));
            hooks.push(hook(&event, &matcher, &command, target.scope));
        }
        HookEdit::Remove { event, stored } => {
            hooks.retain(|hook| hook.event != event || hook.stored.as_ref() != Some(&stored));
        }
    }
}

fn hook(event: &str, matcher: &str, command: &str, scope: HookScope) -> Hook {
    let source = match scope {
        HookScope::Project => "Local settings (.claude/settings.local.json)",
        HookScope::SharedProject => "Project settings (.claude/settings.json)",
        HookScope::User => "User settings (~/.claude/settings.json)",
    };
    Hook {
        event: event.into(),
        matcher: matcher.into(),
        source: source.into(),
        text: command.into(),
        kind: "command".into(),
        disabled: false,
        stored: Some(StoredHook {
            scope,
            matcher: matcher.into(),
            config: json!({ "type": "command", "command": command }),
        }),
    }
}

/// A hook in each settings file, and one from a plugin that can't be
/// removed here.
fn preview_hooks() -> Hooks {
    let event = |name: &str, summary: &str, takes_matcher| HookEvent {
        name: name.into(),
        summary: summary.into(),
        takes_matcher,
    };
    let plugin = Hook {
        source: "Plugin (formatter)".into(),
        stored: None,
        ..hook("PostToolUse", "Edit|Write", "cargo fmt", HookScope::Project)
    };
    Hooks {
        hooks: vec![
            hook(
                "PreToolUse",
                "Bash",
                "./scripts/guard.sh",
                HookScope::SharedProject,
            ),
            plugin,
            hook("Stop", "", "say \"Claude is done\"", HookScope::User),
            hook(
                "Notification",
                "",
                "terminal-notifier -message ready",
                HookScope::Project,
            ),
        ],
        events: vec![
            event("PreToolUse", "Before tool execution", true),
            event("PostToolUse", "After tool execution", true),
            event("Notification", "When notifications are sent", true),
            event("UserPromptSubmit", "When the user submits a prompt", false),
            event("Stop", "Right before Claude concludes its response", false),
            event("SessionStart", "When a new session is started", true),
        ],
        locked: None,
    }
}

/// One skill per source, with one locked by its plugin and one turned off.
fn preview_skills() -> Vec<Skill> {
    let skill = |name: &str, source: &str, tokens: u64, state: SkillState| Skill {
        name: name.into(),
        display_name: name.rsplit(':').next().unwrap_or(name).into(),
        description: String::new(),
        source: source.into(),
        tokens,
        state,
        locked_by: None,
        advertised: state != SkillState::Off,
    };
    let mut plugin = skill("docs:style-guide", "plugin", 212, SkillState::On);
    plugin.locked_by = Some(SkillLock::Plugin);
    vec![
        skill("code-review", "user", 148, SkillState::On),
        skill("release-notes", "project", 64, SkillState::NameOnly),
        plugin,
        skill("draft-email", "user", 51, SkillState::Off),
    ]
}

fn set_skill_state(skills: &mut [Skill], name: &str, state: SkillState) {
    for skill in skills.iter_mut().filter(|skill| skill.name == name) {
        skill.state = state;
        skill.advertised = state != SkillState::Off;
    }
}
