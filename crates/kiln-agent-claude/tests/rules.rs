//! Replays the captured permission rules listing, and a rule removed by
//! Kiln from its settings file before the listing is asked for again.

// Inside a `cfg(test)` module so helpers may `unwrap`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod rules {
    use kiln_agent::{
        AgentBackend as _, AgentEvent, PermissionRule, RuleBehavior, RuleSource, SessionCommand,
        SessionSpec,
    };
    use kiln_agent_claude::{ClaudeBackend, Translator};

    use super::common::{fixture_path, output_lines};

    fn listings(events: impl IntoIterator<Item = AgentEvent>) -> Vec<Vec<PermissionRule>> {
        events
            .into_iter()
            .filter_map(|event| match event {
                AgentEvent::Rules(rules) => Some(rules),
                _ => None,
            })
            .collect()
    }

    fn names(rules: &[PermissionRule]) -> Vec<&str> {
        rules.iter().map(|rule| rule.rule.as_str()).collect()
    }

    #[test]
    fn rules_are_listed_with_their_reading_and_source() {
        let mut translator = Translator::default();
        let request = translator.on_command(SessionCommand::RefreshRules).unwrap();
        let recorded = std::fs::read_to_string(fixture_path("rules")).unwrap();
        let sent = recorded
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .any(|record| record["dir"] == "in" && record["msg"] == request);
        assert!(sent, "{request} was not recorded");
        let listed = listings(
            output_lines("rules")
                .iter()
                .flat_map(|line| translator.on_line(line)),
        );
        let first = &listed[0];
        assert_eq!(
            first[0],
            PermissionRule {
                rule: "Bash(npm test:*)".into(),
                behavior: RuleBehavior::Allow,
                description: Some("Any Bash command starting with npm test".into()),
                source: RuleSource::Project,
            }
        );
        assert_eq!(
            first[1].description.as_deref(),
            Some("Any use of the WebFetch tool")
        );
        assert_eq!(first[2].behavior, RuleBehavior::Deny);
        assert_eq!(first[2].description, None);
    }

    /// The fake CLI answers the second listing as the real one did once it
    /// had read the edited file.
    #[test]
    fn a_removed_rule_leaves_its_file_and_the_listing() {
        let dir = tempfile::tempdir().unwrap();
        let settings = dir.path().join(".claude/settings.local.json");
        std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
        let kept =
            r#"{"permissions":{"allow":["Bash(npm test:*)","WebFetch"],"deny":["Read(./.env)"]}}"#;
        std::fs::write(&settings, kept).unwrap();
        let kiln_agent::Session {
            events,
            commands,
            driver,
        } = ClaudeBackend::new(env!("CARGO_BIN_EXE_fake-claude"))
            .spawn(SessionSpec {
                cwd: dir.path().to_path_buf(),
                env: vec![(
                    "KILN_FAKE_CLAUDE_FIXTURE".into(),
                    fixture_path("rules").to_string_lossy().into_owned(),
                )],
                model: None,
                mode: None,
                resume: None,
                fork: false,
            })
            .unwrap();
        let driver = std::thread::spawn(move || futures::executor::block_on(driver));
        commands
            .send_blocking(SessionCommand::RefreshRules)
            .unwrap();
        let mut seen = Vec::new();
        while let Ok(event) = events.recv_blocking() {
            if let AgentEvent::Rules(rules) = &event
                && seen.is_empty()
            {
                let remove = SessionCommand::RemoveRule(rules[0].clone());
                commands.send_blocking(remove).unwrap();
            }
            if let AgentEvent::Rules(_) = &event {
                seen.push(event);
            } else if let AgentEvent::RuleRemovalFailed { error, .. } = &event {
                panic!("{error}");
            }
        }
        driver.join().unwrap();
        let listed = listings(seen);
        assert_eq!(
            names(&listed[0]),
            ["Bash(npm test:*)", "WebFetch", "Read(./.env)"]
        );
        assert_eq!(names(&listed[1]), ["WebFetch", "Read(./.env)"]);
        let text = std::fs::read_to_string(&settings).unwrap();
        assert!(!text.contains("npm test"), "{text}");
        assert!(text.contains("WebFetch") && text.contains("Read(./.env)"));
    }
}
