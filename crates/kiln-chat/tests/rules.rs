//! The permission rules button in the composer strip and the panel it
//! opens: the rules by what they do, and removing one kept in a file.

// Inside a `cfg(test)` module so helpers may `expect`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod rules {
    use gpui_kit::TestAppContext;
    use gpui_kit::test::{TestQueryExt, TestWindowExt};
    use kiln_agent::{
        AgentEvent, PermissionMode, PermissionRule, RuleBehavior, RuleSource, SessionCommand,
        SessionInfo,
    };
    use kiln_chat::Status;

    use super::common::Harness;

    fn rule(rule: &str, behavior: RuleBehavior, source: RuleSource) -> PermissionRule {
        PermissionRule {
            rule: rule.into(),
            behavior,
            description: Some(format!("{rule} for tests")),
            source,
        }
    }

    fn web() -> PermissionRule {
        rule("WebFetch", RuleBehavior::Allow, RuleSource::User)
    }

    fn listing() -> AgentEvent {
        AgentEvent::Rules(vec![
            web(),
            rule("Bash(ls:*)", RuleBehavior::Allow, RuleSource::Session),
            rule("Read(./.env)", RuleBehavior::Deny, RuleSource::Project),
        ])
    }

    /// A chat with the rules panel open, before its first listing.
    fn opening(cx: &mut TestAppContext) -> Harness {
        let harness = Harness::new(cx);
        harness.emit(
            cx,
            AgentEvent::Ready(SessionInfo {
                mode: PermissionMode::AskBeforeEdits,
                ..SessionInfo::default()
            }),
        );
        harness.commands();
        click(&harness, cx, "chat-rules");
        assert_eq!(harness.commands(), [SessionCommand::RefreshRules]);
        assert!(shown(&harness, cx, "chat-rules-loading"));
        harness
    }

    fn opened(cx: &mut TestAppContext) -> Harness {
        let harness = opening(cx);
        harness.emit(cx, listing());
        harness
    }

    fn shown(harness: &Harness, cx: &mut TestAppContext, id: &'static str) -> bool {
        harness.with_window(cx, |window, _| window.try_find(id).is_some())
    }

    fn click(harness: &Harness, cx: &mut TestAppContext, id: &'static str) {
        harness.with_window(cx, |window, cx| window.click(id, cx));
    }

    #[gpui_kit::test]
    fn the_button_opens_the_rules_denied_first(cx: &mut TestAppContext) {
        let harness = opened(cx);
        assert!(!shown(&harness, cx, "chat-rules-loading"));
        harness.with_window(cx, |window, _| {
            let denied = window.find_by_label("Denied").bounds();
            let allowed = window.find_by_label("Allowed").bounds();
            assert!(denied.origin.y < allowed.origin.y);
            assert!(
                window
                    .find_by_label("WebFetch for tests · All projects")
                    .visible()
            );
        });
        assert!(shown(&harness, cx, "rule-remove-allow-user-webfetch"));
        assert!(shown(&harness, cx, "rule-remove-deny-project-read(./.env)"));
        assert!(
            !shown(&harness, cx, "rule-remove-allow-session-bash(ls:*)"),
            "a session rule cannot be removed from a file"
        );
        assert!(shown(&harness, cx, "rule-allow-session-bash(ls:*)"));
        assert_eq!(harness.status(cx), Status::Idle, "no turn started");
    }

    #[gpui_kit::test]
    fn a_removed_rule_leaves_the_panel(cx: &mut TestAppContext) {
        let harness = opened(cx);
        click(&harness, cx, "rule-remove-allow-user-webfetch");
        assert_eq!(harness.commands(), [SessionCommand::RemoveRule(web())]);
        assert!(!shown(&harness, cx, "rule-allow-user-webfetch"));
        assert!(
            shown(&harness, cx, "chat-rules-panel"),
            "the panel stays open"
        );
    }

    #[gpui_kit::test]
    fn a_failed_removal_says_why_and_keeps_the_rule(cx: &mut TestAppContext) {
        let harness = opened(cx);
        click(&harness, cx, "rule-remove-allow-user-webfetch");
        harness.commands();
        harness.emit(
            cx,
            AgentEvent::RuleRemovalFailed {
                rule: "WebFetch".into(),
                error: "settings.json is not valid JSON".into(),
            },
        );
        harness.with_window(cx, |window, _| {
            window.find_by_label("Couldn't remove WebFetch: settings.json is not valid JSON");
        });
        assert!(shown(&harness, cx, "rule-allow-user-webfetch"));
    }

    #[gpui_kit::test]
    fn rules_can_be_read_while_a_turn_runs(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.type_and_send(cx, "run the tests");
        assert_eq!(harness.status(cx), Status::Running);
        harness.commands();
        click(&harness, cx, "chat-rules");
        assert_eq!(harness.commands(), [SessionCommand::RefreshRules]);
    }

    #[gpui_kit::test]
    fn no_rules_says_how_to_add_one(cx: &mut TestAppContext) {
        let harness = opening(cx);
        harness.emit(cx, AgentEvent::Rules(Vec::new()));
        assert!(shown(&harness, cx, "chat-rules-empty"));
    }
}
