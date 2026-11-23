//! The session's permission rules (`list_permission_rules`, what the CLI's
//! `/permissions` lists) and removing one from its settings file. The CLI
//! has no request that changes rules, so Kiln edits the file and the CLI's
//! settings watcher picks the change up.

use std::path::{Path, PathBuf};

use anyhow::Context as _;
use kiln_agent::{PermissionRule, RuleBehavior, RuleSource};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Reply {
    state: State,
}

#[derive(Deserialize)]
struct State {
    rules: Vec<WireRule>,
}

#[derive(Deserialize)]
struct WireRule {
    behavior: String,
    source: String,
    rule: String,
    description: Option<Description>,
    #[serde(default, rename = "notInEffect")]
    not_in_effect: bool,
}

/// The CLI splits its reading of a rule so the rule's own words can be
/// emphasised; Kiln shows it as one line.
#[derive(Deserialize)]
struct Description {
    prefix: String,
    #[serde(default)]
    emphasis: String,
    #[serde(default)]
    suffix: String,
}

pub fn from_reply(response: Value) -> Option<Vec<PermissionRule>> {
    let reply: Reply = serde_json::from_value(response)
        .map_err(|error| log::warn!("unreadable permission rules reply: {error}"))
        .ok()?;
    let rules = reply.state.rules.into_iter().filter_map(|wire| {
        let behavior = match wire.behavior.as_str() {
            "allow" => RuleBehavior::Allow,
            "ask" => RuleBehavior::Ask,
            "deny" => RuleBehavior::Deny,
            _ => return None,
        };
        // Managed settings that allow their own rules only leave the others
        // listed but ignored; they cannot be acted on here either.
        let source = if wire.not_in_effect {
            RuleSource::Fixed
        } else {
            source(&wire.source)
        };
        Some(PermissionRule {
            rule: wire.rule,
            behavior,
            description: wire
                .description
                .map(|text| format!("{}{}{}", text.prefix, text.emphasis, text.suffix)),
            source,
        })
    });
    Some(rules.collect())
}

fn source(name: &str) -> RuleSource {
    match name {
        "session" | "cliArg" => RuleSource::Session,
        "localSettings" => RuleSource::Project,
        "projectSettings" => RuleSource::SharedProject,
        "userSettings" => RuleSource::User,
        _ => RuleSource::Fixed,
    }
}

/// The settings files rules are kept in, for one session.
#[derive(Clone, Debug)]
pub struct RuleFiles {
    /// The CLI's config directory, which holds the user's settings; `None`
    /// without a home directory.
    pub config: Option<PathBuf>,
    pub cwd: PathBuf,
}

impl RuleFiles {
    fn path(&self, source: RuleSource) -> Option<PathBuf> {
        match source {
            RuleSource::Project => Some(self.cwd.join(".claude/settings.local.json")),
            RuleSource::SharedProject => Some(self.cwd.join(".claude/settings.json")),
            RuleSource::User => self.config.as_ref().map(|dir| dir.join("settings.json")),
            RuleSource::Session | RuleSource::Fixed => None,
        }
    }

    /// Takes `rule` out of its settings file. A rule already gone is not an
    /// error: the file may have been edited since it was listed.
    pub fn remove(&self, rule: &PermissionRule) -> anyhow::Result<()> {
        let path = self
            .path(rule.source)
            .context("this rule is not kept in a settings file")?;
        let Some(mut settings) = read(&path)? else {
            return Ok(());
        };
        let Some(list) = rules_in(&mut settings, rule.behavior) else {
            return Ok(());
        };
        let before = list.len();
        list.retain(|kept| kept.as_str() != Some(rule.rule.as_str()));
        if list.len() == before {
            return Ok(());
        }
        write(&path, &settings)
    }

    /// `rules` without those Kiln removed from a file that the CLI has not
    /// read again yet: it notices a change only after a few seconds.
    #[must_use]
    pub fn still_kept(&self, rules: Vec<PermissionRule>) -> Vec<PermissionRule> {
        rules
            .into_iter()
            .filter(|rule| {
                let Some(path) = self.path(rule.source) else {
                    return true;
                };
                let Ok(Some(mut settings)) = read(&path) else {
                    return true;
                };
                rules_in(&mut settings, rule.behavior).is_some_and(|list| {
                    list.iter()
                        .any(|kept| kept.as_str() == Some(rule.rule.as_str()))
                })
            })
            .collect()
    }
}

/// `permissions.allow` (or `ask`, `deny`) in a settings file.
fn rules_in(settings: &mut Value, behavior: RuleBehavior) -> Option<&mut Vec<Value>> {
    let key = match behavior {
        RuleBehavior::Allow => "allow",
        RuleBehavior::Ask => "ask",
        RuleBehavior::Deny => "deny",
    };
    settings
        .get_mut("permissions")?
        .get_mut(key)?
        .as_array_mut()
}

/// The settings in `path`; `None` when there is no such file.
fn read(path: &Path) -> anyhow::Result<Option<Value>> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("cannot read {}", path.display())),
    };
    let settings: Value = serde_json::from_str(&text)
        .with_context(|| format!("{} is not valid JSON", path.display()))?;
    Ok(settings.is_object().then_some(settings))
}

/// Writes the settings back in the CLI's own layout, keeping key order, and
/// renames the result over the file so a crash leaves the previous one.
fn write(path: &Path, settings: &Value) -> anyhow::Result<()> {
    let mut text = serde_json::to_string_pretty(settings)?;
    text.push('\n');
    let temporary = path.with_extension("json.kiln-tmp");
    std::fs::write(&temporary, text)
        .with_context(|| format!("cannot write {}", temporary.display()))?;
    std::fs::rename(&temporary, path).with_context(|| format!("cannot write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(dir: &Path) -> RuleFiles {
        RuleFiles {
            config: Some(dir.join("config")),
            cwd: dir.join("project"),
        }
    }

    fn rule(rule: &str, behavior: RuleBehavior, source: RuleSource) -> PermissionRule {
        PermissionRule {
            rule: rule.into(),
            behavior,
            description: None,
            source,
        }
    }

    fn settings_file(dir: &Path, text: &str) -> PathBuf {
        let path = dir.join("project/.claude/settings.local.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn removing_a_rule_keeps_the_rest_of_the_file_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let path = settings_file(
            dir.path(),
            r#"{"theme":"dark","permissions":{"deny":["Read(./.env)"],"allow":["Bash(npm test:*)","WebFetch"]},"model":"opus"}"#,
        );
        let gone = rule("Bash(npm test:*)", RuleBehavior::Allow, RuleSource::Project);
        files(dir.path()).remove(&gone).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let expected = r#"{
  "theme": "dark",
  "permissions": {
    "deny": [
      "Read(./.env)"
    ],
    "allow": [
      "WebFetch"
    ]
  },
  "model": "opus"
}
"#;
        assert_eq!(text, expected);
    }

    #[test]
    fn a_rule_already_gone_leaves_the_file_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let text = r#"{"permissions":{"allow":["WebFetch"]}}"#;
        let path = settings_file(dir.path(), text);
        let files = files(dir.path());
        let deny = rule("WebFetch", RuleBehavior::Deny, RuleSource::Project);
        files.remove(&deny).unwrap();
        let user = rule("WebFetch", RuleBehavior::Allow, RuleSource::User);
        files.remove(&user).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
        let session = rule("WebFetch", RuleBehavior::Allow, RuleSource::Session);
        assert!(files.remove(&session).is_err());
    }

    #[test]
    fn a_damaged_file_is_reported_not_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let path = settings_file(dir.path(), "{ \"permissions\": ");
        let gone = rule("WebFetch", RuleBehavior::Allow, RuleSource::Project);
        assert!(files(dir.path()).remove(&gone).is_err());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{ \"permissions\": "
        );
    }

    #[test]
    fn rules_removed_from_their_file_are_not_listed_again() {
        let dir = tempfile::tempdir().unwrap();
        settings_file(dir.path(), r#"{"permissions":{"allow":["WebFetch"]}}"#);
        let listed = vec![
            rule("Bash(npm test:*)", RuleBehavior::Allow, RuleSource::Project),
            rule("WebFetch", RuleBehavior::Allow, RuleSource::Project),
            rule("Bash(ls:*)", RuleBehavior::Allow, RuleSource::Session),
        ];
        let kept: Vec<String> = files(dir.path())
            .still_kept(listed)
            .into_iter()
            .map(|rule| rule.rule)
            .collect();
        assert_eq!(kept, ["WebFetch", "Bash(ls:*)"]);
    }
}
