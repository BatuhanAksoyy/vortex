//! The skills button in the composer strip and the panel it opens: the
//! skills by source, and changing one's state from its menu.

// Inside a `cfg(test)` module so helpers may `expect`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod skills {
    use gpui_kit::TestAppContext;
    use gpui_kit::test::{TestQueryExt, TestWindowExt};
    use kiln_agent::{
        AgentEvent, PermissionMode, SessionCommand, SessionInfo, Skill, SkillLock, SkillState,
    };
    use kiln_chat::Status;

    use super::common::Harness;

    fn skill(name: &str, source: &str, state: SkillState) -> Skill {
        Skill {
            name: name.into(),
            display_name: name.into(),
            description: format!("{name} for tests"),
            source: source.into(),
            tokens: 40,
            state,
            locked_by: None,
            advertised: state != SkillState::Off,
        }
    }

    fn listing(review: SkillState) -> AgentEvent {
        let mut docs = skill("docs", "plugin", SkillState::On);
        docs.locked_by = Some(SkillLock::Plugin);
        AgentEvent::Skills(vec![
            skill("review", "project", review),
            skill("draft", "project", SkillState::Off),
            docs,
        ])
    }

    /// A chat with the skills panel open and its first listing in.
    fn opened(cx: &mut TestAppContext) -> Harness {
        let harness = Harness::new(cx);
        harness.emit(
            cx,
            AgentEvent::Ready(SessionInfo {
                mode: PermissionMode::AskBeforeEdits,
                ..SessionInfo::default()
            }),
        );
        harness.commands();
        click(&harness, cx, "chat-skills");
        assert_eq!(harness.commands(), [SessionCommand::RefreshSkills]);
        assert!(shown(&harness, cx, "chat-skills-loading"));
        harness.emit(cx, listing(SkillState::On));
        harness
    }

    fn shown(harness: &Harness, cx: &mut TestAppContext, id: &'static str) -> bool {
        harness.with_window(cx, |window, _| window.try_find(id).is_some())
    }

    fn click(harness: &Harness, cx: &mut TestAppContext, id: &'static str) {
        harness.with_window(cx, |window, cx| window.click(id, cx));
    }

    fn set(name: &str, state: SkillState) -> SessionCommand {
        SessionCommand::SetSkillState {
            name: name.into(),
            state,
        }
    }

    #[gpui_kit::test]
    fn the_button_opens_the_skills_by_source(cx: &mut TestAppContext) {
        let harness = opened(cx);
        assert!(shown(&harness, cx, "chat-skills-panel"));
        assert!(!shown(&harness, cx, "chat-skills-loading"));
        harness.with_window(cx, |window, _| {
            assert!(window.find_by_label("Project").visible());
            assert!(window.find_by_label("Plugin").visible());
            let project = window.find("skills-source-project").bounds();
            let plugin = window.find("skills-source-plugin").bounds();
            assert!(project.origin.y < plugin.origin.y, "in the agent's order");
        });
        assert!(shown(&harness, cx, "skill-state-review"));
        assert!(
            shown(&harness, cx, "skill-state-draft"),
            "an off skill still lists"
        );
        assert!(
            !shown(&harness, cx, "skill-state-docs"),
            "a locked skill has no menu"
        );
        assert!(shown(&harness, cx, "skill-lock-docs"));
        assert_eq!(harness.status(cx), Status::Idle, "no turn started");
    }

    #[gpui_kit::test]
    fn a_state_is_chosen_from_the_skills_menu(cx: &mut TestAppContext) {
        let harness = opened(cx);
        click(&harness, cx, "skill-state-review");
        click(&harness, cx, "skill-state-review-UserOnly");
        assert_eq!(harness.commands(), [set("review", SkillState::UserOnly)]);
        assert!(
            shown(&harness, cx, "chat-skills-panel"),
            "the panel stays open"
        );
        harness.emit(cx, listing(SkillState::UserOnly));
        click(&harness, cx, "skill-state-review");
        click(&harness, cx, "skill-state-review-On");
        assert_eq!(harness.commands(), [set("review", SkillState::On)]);
    }

    #[gpui_kit::test]
    fn a_failed_save_says_why(cx: &mut TestAppContext) {
        let harness = opened(cx);
        click(&harness, cx, "skill-state-review");
        click(&harness, cx, "skill-state-review-Off");
        harness.commands();
        harness.emit(
            cx,
            AgentEvent::SkillStateFailed {
                name: "review".into(),
                error: "settings are read-only".into(),
            },
        );
        harness.emit(cx, listing(SkillState::On));
        harness.with_window(cx, |window, _| {
            window.find_by_label("The skill state was not saved: settings are read-only");
        });
        click(&harness, cx, "skill-state-review");
        click(&harness, cx, "skill-state-review-Off");
        assert_eq!(harness.commands(), [set("review", SkillState::Off)]);
    }

    #[gpui_kit::test]
    fn escape_closes_the_panel(cx: &mut TestAppContext) {
        let harness = opened(cx);
        harness.with_window(cx, |window, cx| window.press("escape", cx));
        assert!(!shown(&harness, cx, "chat-skills-panel"));
    }

    #[gpui_kit::test]
    fn skills_is_no_command_of_kiln(cx: &mut TestAppContext) {
        let harness = Harness::new(cx);
        harness.type_and_send(cx, "/skills");
        assert!(!shown(&harness, cx, "chat-skills-panel"));
    }
}
