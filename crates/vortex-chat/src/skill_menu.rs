//! The skills panel as data: the skills by source, and a state change from
//! the choice until the agent confirms it. Kept free of GPUI so the rules
//! are unit-tested directly.

use vortex_agent::{Skill, SkillState};

/// How many times a saved state is read back before the panel says the
/// session has not picked it up yet.
const CHECKS: u8 = 5;

const NOT_PICKED_UP: &str = "Saved, but this session still shows the previous state. It may not \
                             have picked the change up yet; reopen the panel to check.";

#[derive(Default)]
pub struct SkillMenu {
    /// `None` until the agent lists them.
    skills: Option<Vec<Skill>>,
    saving: Option<Saving>,
    error: Option<String>,
    notice: Option<String>,
}

struct Saving {
    name: String,
    state: SkillState,
    checks: u8,
    /// Set once the edit is known to have failed.
    failed: bool,
}

impl Saving {
    fn shown_in(&self, skills: &[Skill]) -> bool {
        skills
            .iter()
            .any(|skill| skill.name == self.name && skill.state == self.state)
    }
}

impl SkillMenu {
    pub fn saving(&self) -> Option<&str> {
        self.saving.as_ref().map(|saving| saving.name.as_str())
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    pub fn is_loaded(&self) -> bool {
        self.skills.is_some()
    }

    /// The skills in the agent's order, which keeps each source together,
    /// as runs of one source each.
    pub fn by_source(&self) -> Vec<(&str, Vec<&Skill>)> {
        let mut groups: Vec<(&str, Vec<&Skill>)> = Vec::new();
        for skill in self.skills.as_deref().unwrap_or_default() {
            match groups.last_mut() {
                Some((source, skills)) if *source == skill.source => skills.push(skill),
                _ => groups.push((&skill.source, vec![skill])),
            }
        }
        groups
    }

    /// Shows `state` on the skill `name` at once; `false` while another
    /// save runs, or when the skill is locked, unknown or already there.
    pub fn begin_save(&mut self, name: &str, state: SkillState) -> bool {
        if self.saving.is_some() {
            return false;
        }
        let Some(skill) = self.skills.as_mut().and_then(|skills| {
            skills
                .iter_mut()
                .find(|skill| skill.name == name && skill.locked_by.is_none())
        }) else {
            return false;
        };
        if skill.state == state {
            return false;
        }
        skill.state = state;
        self.saving = Some(Saving {
            name: name.to_owned(),
            state,
            checks: 0,
            failed: false,
        });
        self.error = None;
        self.notice = None;
        true
    }

    /// The save of `name` failed; the next listing shows the state as it is.
    pub fn fail(&mut self, name: &str, error: &str) {
        if let Some(saving) = self.saving.as_mut().filter(|saving| saving.name == name) {
            saving.failed = true;
        }
        self.error = Some(format!("The skill state was not saved: {error}"));
    }

    /// Takes a listing from the agent; `true` when it should be read again
    /// because a saved state has not shown up yet. Until then the row keeps
    /// the state just chosen.
    pub fn apply(&mut self, skills: Vec<Skill>) -> bool {
        let unconfirmed = self
            .saving
            .as_mut()
            .filter(|saving| !saving.failed && !saving.shown_in(&skills));
        if let Some(saving) = unconfirmed {
            saving.checks += 1;
            if saving.checks < CHECKS {
                return true;
            }
            self.notice = Some(NOT_PICKED_UP.to_owned());
        }
        self.saving = None;
        self.skills = Some(skills);
        false
    }

    /// Forgets messages from an earlier opening.
    pub fn clear_messages(&mut self) {
        self.error = None;
        self.notice = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vortex_agent::SkillLock;

    fn skill(name: &str, source: &str, state: SkillState) -> Skill {
        Skill {
            name: name.into(),
            display_name: name.into(),
            description: String::new(),
            source: source.into(),
            tokens: 10,
            state,
            locked_by: None,
            advertised: state != SkillState::Off,
        }
    }

    fn review(state: SkillState) -> Skill {
        skill("review", "project", state)
    }

    fn loaded(skills: Vec<Skill>) -> SkillMenu {
        let mut menu = SkillMenu::default();
        assert!(!menu.apply(skills));
        menu
    }

    fn shows(menu: &SkillMenu, name: &str, state: SkillState) -> bool {
        menu.by_source()
            .into_iter()
            .flat_map(|(_, skills)| skills)
            .find(|skill| skill.name == name)
            .is_some_and(|skill| skill.state == state)
    }

    #[test]
    fn skills_are_grouped_by_their_source_in_order() {
        let menu = loaded(vec![
            skill("a", "user", SkillState::On),
            skill("b", "user", SkillState::Off),
            skill("c", "project", SkillState::On),
        ]);
        let groups: Vec<(&str, usize)> = menu
            .by_source()
            .into_iter()
            .map(|(source, skills)| (source, skills.len()))
            .collect();
        assert_eq!(groups, [("user", 2), ("project", 1)]);
    }

    #[test]
    fn a_choice_shows_at_once_and_holds_until_the_agent_confirms_it() {
        let mut menu = loaded(vec![review(SkillState::On)]);
        assert!(menu.begin_save("review", SkillState::Off));
        assert!(
            !menu.begin_save("review", SkillState::On),
            "one save at a time"
        );
        assert_eq!(menu.saving(), Some("review"));
        assert!(menu.apply(vec![review(SkillState::On)]), "read again");
        assert!(
            shows(&menu, "review", SkillState::Off),
            "kept while checking"
        );
        assert!(!menu.apply(vec![review(SkillState::Off)]));
        assert_eq!(menu.saving(), None);
        assert_eq!(menu.notice(), None);
    }

    #[test]
    fn the_current_state_is_no_change() {
        let mut menu = loaded(vec![review(SkillState::On)]);
        assert!(!menu.begin_save("review", SkillState::On));
        assert_eq!(menu.saving(), None);
    }

    #[test]
    fn a_state_that_never_shows_up_ends_in_a_notice() {
        let mut menu = loaded(vec![review(SkillState::On)]);
        menu.begin_save("review", SkillState::UserOnly);
        let mut checks = 0;
        while menu.apply(vec![review(SkillState::On)]) {
            checks += 1;
        }
        assert_eq!(checks, CHECKS - 1);
        assert_eq!(menu.saving(), None);
        assert!(menu.notice().is_some_and(|text| text.starts_with("Saved")));
    }

    #[test]
    fn a_failed_save_shows_the_error_and_the_real_state() {
        let mut menu = loaded(vec![review(SkillState::On)]);
        menu.begin_save("review", SkillState::Off);
        menu.fail("review", "no settings");
        assert!(!menu.apply(vec![review(SkillState::On)]));
        assert_eq!(
            menu.error(),
            Some("The skill state was not saved: no settings")
        );
        assert!(shows(&menu, "review", SkillState::On));
    }

    #[test]
    fn a_locked_skill_does_not_change() {
        let mut locked = review(SkillState::On);
        locked.locked_by = Some(SkillLock::Plugin);
        let mut menu = loaded(vec![locked]);
        assert!(!menu.begin_save("review", SkillState::Off));
    }
}
