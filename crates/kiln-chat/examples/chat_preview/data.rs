//! The canned agent state the preview shows: context, commands, rules and
//! skills.

use kiln_agent::{
    AgentEvent, ContextCategory, ContextKind, ContextUsage, PermissionRule, RuleBehavior,
    RuleSource, SessionCommand, Skill, SkillLock, SkillState, SlashCommand,
};

/// The skills and rules the preview's panels list and change.
pub struct Settings {
    skills: Vec<Skill>,
    rules: Vec<PermissionRule>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            skills: preview_skills(),
            rules: preview_rules(),
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
            SessionCommand::RemoveRule(rule) => self.rules.retain(|kept| *kept != rule),
            SessionCommand::RefreshSkills | SessionCommand::RefreshRules => {}
            _ => return Vec::new(),
        }
        vec![
            AgentEvent::Skills(self.skills.clone()),
            AgentEvent::Rules(self.rules.clone()),
        ]
    }
}

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

/// One skill per source, with one locked by its plugin and one turned off.
/// A second account beside the default one, kept out of the user's list.
fn preview_rules() -> Vec<PermissionRule> {
    let rule = |rule: &str, behavior, description: Option<&str>, source| PermissionRule {
        rule: rule.into(),
        behavior,
        description: description.map(Into::into),
        source,
    };
    vec![
        rule(
            "Bash(npm test:*)",
            RuleBehavior::Allow,
            Some("Any Bash command starting with npm test"),
            RuleSource::Project,
        ),
        rule(
            "WebFetch",
            RuleBehavior::Allow,
            Some("Any use of the WebFetch tool"),
            RuleSource::User,
        ),
        rule("Bash(ls:*)", RuleBehavior::Allow, None, RuleSource::Session),
        rule(
            "Read(./.env)",
            RuleBehavior::Deny,
            None,
            RuleSource::SharedProject,
        ),
    ]
}

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
