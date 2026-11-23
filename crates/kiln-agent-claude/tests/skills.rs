//! Replays the captured `/skills` menu: the rows Kiln shows, and the edit a
//! state change sends back.

// Inside a `cfg(test)` module so helpers may `unwrap`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod skills {
    use kiln_agent::{AgentEvent, SessionCommand, Skill, SkillLock, SkillState};
    use kiln_agent_claude::Translator;
    use serde_json::json;

    use super::common::{fixture_path, output_lines};

    /// Asks for the menu as the capture did, then replays the reply.
    fn listed() -> (Translator, Vec<Skill>) {
        let mut translator = Translator::default();
        let request = translator
            .on_command(SessionCommand::RefreshSkills)
            .unwrap();
        let recorded = std::fs::read_to_string(fixture_path("skills")).unwrap();
        let sent = recorded
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .any(|record| record["dir"] == "in" && record["msg"] == request);
        assert!(sent, "{request} was not recorded");
        let skills = output_lines("skills")
            .iter()
            .flat_map(|line| translator.on_line(line))
            .find_map(|event| match event {
                AgentEvent::Skills(skills) => Some(skills),
                _ => None,
            })
            .unwrap();
        (translator, skills)
    }

    fn named<'a>(skills: &'a [Skill], name: &str) -> &'a Skill {
        skills.iter().find(|skill| skill.name == name).unwrap()
    }

    #[test]
    fn the_menu_lists_each_skill_with_its_state() {
        let (_, skills) = listed();
        let shown = named(&skills, "kiln-fixture");
        assert_eq!(shown.source, "project");
        assert_eq!(shown.state, SkillState::On);
        assert!(shown.advertised);
        assert!(shown.tokens > 0);
        let hidden = named(&skills, "kiln-hidden");
        assert_eq!(hidden.state, SkillState::Off);
        assert!(!hidden.advertised, "a skill that is off is no command");
        let plugin = skills
            .iter()
            .find(|skill| skill.source == "plugin")
            .unwrap();
        assert_eq!(plugin.locked_by, Some(SkillLock::Plugin));
        assert_eq!(plugin.display_name, "example-2");
    }

    #[test]
    fn an_edit_sends_back_the_names_the_menu_reported() {
        let (translator, _) = listed();
        assert_eq!(
            translator.skill_edit("example-source:example-1", SkillState::NameOnly),
            json!({
                "name": "example-source:example-1",
                "state": "name-only",
                "handles": { "unqualified_name": "example-1", "aliases": ["example-1"] },
            })
        );
        assert_eq!(
            translator.skill_edit("kiln-fixture", SkillState::Off),
            json!({ "name": "kiln-fixture", "state": "off", "handles": {} })
        );
    }

    #[test]
    fn a_state_change_is_no_message_to_the_session() {
        let mut translator = Translator::default();
        let change = SessionCommand::SetSkillState {
            name: "kiln-fixture".into(),
            state: SkillState::Off,
        };
        assert_eq!(translator.on_command(change), None);
    }
}

/// Talks to the installed `claude` CLI; spends no usage. The CLI refuses the
/// edit from inside a Claude Code session, so run it from a plain terminal:
/// `cargo test -p kiln-agent-claude --test skills -- --ignored`.
#[cfg(test)]
mod live {
    use std::{path::Path, time::Duration};

    use kiln_agent::{AgentBackend as _, AgentEvent, SessionCommand, SessionSpec, SkillState};
    use kiln_agent_claude::ClaudeBackend;

    const SKILL: &str = "kiln-live";

    fn project() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("kiln-live-skills-{}", std::process::id()));
        let skill = dir.join(".claude/skills").join(SKILL);
        std::fs::create_dir_all(&skill).unwrap();
        let file = format!("---\nname: {SKILL}\ndescription: A live test skill.\n---\nSay hi.\n");
        std::fs::write(skill.join("SKILL.md"), file).unwrap();
        dir
    }

    fn saved_in(dir: &Path) -> Vec<&'static str> {
        ["settings.json", "settings.local.json"]
            .into_iter()
            .filter(|file| {
                std::fs::read_to_string(dir.join(".claude").join(file))
                    .is_ok_and(|text| text.contains(SKILL))
            })
            .collect()
    }

    #[test]
    #[ignore = "needs the installed `claude`, run outside a Claude Code session"]
    fn live_claude_saves_a_skill_state() {
        let dir = project();
        let spec = SessionSpec {
            cwd: dir.clone(),
            env: Vec::new(),
            model: None,
            mode: None,
        };
        let kiln_agent::Session {
            events,
            commands,
            driver,
        } = ClaudeBackend::new("claude").spawn(spec).unwrap();
        let driver = std::thread::spawn(move || futures::executor::block_on(driver));
        let off = SessionCommand::SetSkillState {
            name: SKILL.into(),
            state: SkillState::Off,
        };
        commands
            .send_blocking(SessionCommand::RefreshSkills)
            .unwrap();
        let mut sent_off = false;
        let mut state = None;
        while let Ok(event) = events.recv_blocking() {
            if let AgentEvent::SkillStateFailed { error, .. } = &event {
                panic!("{error}");
            }
            let AgentEvent::Skills(skills) = event else {
                continue;
            };
            state = skills.iter().find(|s| s.name == SKILL).map(|s| s.state);
            let next = match (sent_off, state) {
                (false, _) => off.clone(),
                (true, Some(SkillState::Off)) => break,
                // The CLI picks a settings change up a moment later.
                (true, _) => {
                    std::thread::sleep(Duration::from_millis(300));
                    SessionCommand::RefreshSkills
                }
            };
            sent_off = true;
            commands.send_blocking(next).unwrap();
        }
        commands.close();
        driver.join().unwrap();
        assert_eq!(state, Some(SkillState::Off));
        // The /skills menu saves to the project's local settings file.
        assert_eq!(saved_in(&dir), ["settings.local.json"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
