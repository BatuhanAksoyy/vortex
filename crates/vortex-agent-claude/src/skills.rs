//! The rows of the CLI's `/skills` menu (`get_skills_dialog`) and the edit
//! that saves a skill's state. The CLI writes the edit itself, through its
//! `edit-skill-overrides` subcommand, as the VS Code extension does.

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::{Value, json};
use vortex_agent::{Skill, SkillLock, SkillState};

#[derive(Deserialize)]
struct WireSkills {
    skills: Vec<WireSkill>,
}

#[derive(Deserialize)]
struct WireSkill {
    name: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    source: String,
    #[serde(default)]
    tokens: u64,
    state: String,
    locked_by: Option<String>,
    #[serde(default)]
    advertised: bool,
    handles: Option<Value>,
}

/// A menu reply: the skills, and the other names each one's state may be
/// saved under, which an edit must send back so it reads the same entries.
pub struct Listed {
    pub skills: Vec<Skill>,
    pub handles: HashMap<String, Value>,
}

pub fn from_reply(response: Value) -> Option<Listed> {
    let reply: WireSkills = serde_json::from_value(response)
        .map_err(|error| log::warn!("unreadable skills reply: {error}"))
        .ok()?;
    let mut handles = HashMap::new();
    let skills = reply
        .skills
        .into_iter()
        .map(|row| {
            if let Some(names) = row.handles {
                handles.insert(row.name.clone(), names);
            }
            Skill {
                display_name: if row.display_name.is_empty() {
                    row.name.clone()
                } else {
                    row.display_name
                },
                name: row.name,
                description: row.description,
                source: row.source,
                tokens: row.tokens,
                state: parse_state(&row.state),
                locked_by: row.locked_by.as_deref().map(parse_lock),
                advertised: row.advertised,
            }
        })
        .collect();
    Some(Listed { skills, handles })
}

/// The edit `edit-skill-overrides` reads from stdin.
pub fn edit(name: &str, state: SkillState, handles: Option<&Value>) -> Value {
    let mut edit = json!({ "name": name, "state": state_name(state) });
    if let Some(handles) = handles {
        edit["handles"] = handles.clone();
    }
    edit
}

pub fn state_name(state: SkillState) -> &'static str {
    match state {
        SkillState::On => "on",
        SkillState::NameOnly => "name-only",
        SkillState::UserOnly => "user-invocable-only",
        SkillState::Off => "off",
    }
}

/// A state this version does not know is shown as on, which is what the
/// CLI assumes when no setting names the skill.
fn parse_state(name: &str) -> SkillState {
    SkillState::ALL
        .into_iter()
        .find(|state| state_name(*state) == name)
        .unwrap_or_default()
}

/// An unknown lock still locks: the CLI would refuse the edit.
fn parse_lock(name: &str) -> SkillLock {
    match name {
        "author" => SkillLock::Author,
        "plugin" => SkillLock::Plugin,
        "reserved-name" => SkillLock::ReservedName,
        _ => SkillLock::Policy,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_state_round_trips_through_its_cli_name() {
        for state in SkillState::ALL {
            assert_eq!(parse_state(state_name(state)), state);
        }
        assert_eq!(parse_state("later-state"), SkillState::On);
    }

    #[test]
    fn an_edit_carries_the_names_the_menu_reported() {
        let handles = json!({ "aliases": ["short"] });
        assert_eq!(
            edit("plugin:short", SkillState::UserOnly, Some(&handles)),
            json!({
                "name": "plugin:short",
                "state": "user-invocable-only",
                "handles": { "aliases": ["short"] },
            })
        );
        assert_eq!(
            edit("mine", SkillState::Off, None),
            json!({ "name": "mine", "state": "off" })
        );
    }
}
