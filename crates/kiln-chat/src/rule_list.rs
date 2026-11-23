//! The permission rules panel as data: the rules by what they do, and a
//! removal from the click until the agent lists the rules again. Kept free
//! of GPUI so the rules are unit-tested directly.

use kiln_agent::{PermissionRule, RuleBehavior};

/// The order groups are shown in: what is refused first.
const ORDER: [RuleBehavior; 3] = [RuleBehavior::Deny, RuleBehavior::Ask, RuleBehavior::Allow];

#[derive(Default)]
pub struct RuleList {
    /// `None` until the agent lists them.
    rules: Option<Vec<PermissionRule>>,
    removing: Option<PermissionRule>,
    error: Option<String>,
}

impl RuleList {
    pub fn is_loaded(&self) -> bool {
        self.rules.is_some()
    }

    pub fn is_removing(&self) -> bool {
        self.removing.is_some()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn clear_messages(&mut self) {
        self.error = None;
    }

    /// Non-empty groups, in [`ORDER`], each in the agent's order.
    pub fn by_behavior(&self) -> Vec<(RuleBehavior, Vec<&PermissionRule>)> {
        let rules = self.rules.as_deref().unwrap_or_default();
        ORDER
            .into_iter()
            .map(|behavior| {
                let group = rules.iter().filter(|rule| rule.behavior == behavior);
                (behavior, group.collect::<Vec<_>>())
            })
            .filter(|(_, group)| !group.is_empty())
            .collect()
    }

    /// Hides `rule` at once; `false` while another removal runs or when the
    /// rule cannot be removed here.
    pub fn begin_remove(&mut self, rule: &PermissionRule) -> bool {
        if self.removing.is_some() || !rule.source.is_removable() {
            return false;
        }
        if let Some(rules) = &mut self.rules {
            rules.retain(|listed| listed != rule);
        }
        self.removing = Some(rule.clone());
        self.error = None;
        true
    }

    pub fn apply(&mut self, rules: Vec<PermissionRule>) {
        self.rules = Some(rules);
        self.removing = None;
    }

    /// Shows the rule again with why it stayed.
    pub fn fail(&mut self, error: &str) {
        let Some(rule) = self.removing.take() else {
            return;
        };
        self.error = Some(format!("Couldn't remove {}: {error}", rule.rule));
        if let Some(rules) = &mut self.rules {
            rules.push(rule);
        }
    }
}

#[cfg(test)]
mod tests {
    use kiln_agent::RuleSource;

    use super::*;

    fn rule(rule: &str, behavior: RuleBehavior, source: RuleSource) -> PermissionRule {
        PermissionRule {
            rule: rule.into(),
            behavior,
            description: None,
            source,
        }
    }

    fn listed() -> RuleList {
        let mut list = RuleList::default();
        list.apply(vec![
            rule("WebFetch", RuleBehavior::Allow, RuleSource::User),
            rule("Read(./.env)", RuleBehavior::Deny, RuleSource::Project),
            rule("Bash(ls:*)", RuleBehavior::Allow, RuleSource::Session),
        ]);
        list
    }

    fn shown(list: &RuleList) -> Vec<(RuleBehavior, Vec<&str>)> {
        list.by_behavior()
            .into_iter()
            .map(|(behavior, rules)| (behavior, rules.iter().map(|r| r.rule.as_str()).collect()))
            .collect()
    }

    #[test]
    fn denied_rules_come_first_and_empty_groups_are_left_out() {
        assert_eq!(
            shown(&listed()),
            [
                (RuleBehavior::Deny, vec!["Read(./.env)"]),
                (RuleBehavior::Allow, vec!["WebFetch", "Bash(ls:*)"]),
            ]
        );
        assert!(!RuleList::default().is_loaded());
    }

    #[test]
    fn a_removed_rule_is_hidden_until_the_removal_fails() {
        let mut list = listed();
        let web = rule("WebFetch", RuleBehavior::Allow, RuleSource::User);
        assert!(list.begin_remove(&web));
        assert!(!list.begin_remove(&rule(
            "Read(./.env)",
            RuleBehavior::Deny,
            RuleSource::Project
        )));
        assert_eq!(shown(&list)[1].1, ["Bash(ls:*)"]);
        list.fail("permission denied");
        assert_eq!(
            list.error(),
            Some("Couldn't remove WebFetch: permission denied")
        );
        assert!(!list.is_removing());
        assert_eq!(shown(&list)[1].1, ["Bash(ls:*)", "WebFetch"]);
    }

    #[test]
    fn session_rules_cannot_be_removed() {
        let mut list = listed();
        let session = rule("Bash(ls:*)", RuleBehavior::Allow, RuleSource::Session);
        assert!(!list.begin_remove(&session));
        assert!(!list.is_removing());
    }
}
